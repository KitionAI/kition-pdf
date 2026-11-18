//! Synchronous compound saving: prepare all expected failures before the writer runs.
use super::*;

pub(super) struct PreparedEdit {
    pub cos: kition_pdf_cos::Document,
    pub keys: Keys,
    pub scope: Scope,
    pub cx: EditCtx,
    pub xfa_notes: Vec<String>,
}

/// Derived state built without replacing the live document or its replica.
pub(super) struct PreparedView {
    bytes: Arc<Vec<u8>>,
    display: Arc<Vec<u8>>,
    info: DocInfo,
    renderer: RenderPool,
    form: Arc<Vec<kition_pdf_forms::Field>>,
    marks: Vec<MarkKind>,
    added: Vec<kition_pdf_edit::Added>,
    links: Vec<kition_pdf_annot::links::LinkItem>,
    signatures: Arc<Vec<SignatureInfo>>,
    xfa: Option<XfaLayout>,
}

impl PreparedView {
    pub(super) fn new(doc: &Document, editor: &Editor, warnings: &[String]) -> Result<Self, EditError> {
        let xfa = if doc.xfa.is_some() { kition_pdf_xfa::existing_layout(&editor.cos).or_else(|| doc.xfa.clone()) } else { None };
        let bytes = if editor.cos.is_modified() {
            Arc::new(write_incremental(&editor.cos, &SaveOptions::default()).map_err(|e| EditError::Write(e.to_string()))?)
        } else {
            editor.cos.bytes().clone()
        };
        // Only reuse authentication in the same credential context. COS additionally requires
        // an unchanged, unrepaired R6 source and reparses it with inspection's tighter limit.
        let inspected = if editor.keys.reopen == editor.keys.render {
            kition_pdf_render::inspect_with_authenticated_source(bytes.clone(), editor.keys.render.as_deref(), &editor.cos)
        } else {
            inspect(bytes.clone(), editor.keys.render.as_deref())
        };
        let mut info = inspected.map_err(|e| EditError::Reopen(e.to_string()))?;
        for layer in &mut info.layers {
            if let Some(old) = doc.info.layers.iter().find(|old| old.id == layer.id) {
                layer.visible = old.visible;
            }
        }
        note_warnings(&mut info.warnings, warnings);
        let mut form = kition_pdf_forms::fields(&editor.cos);
        xfa::mark_script_buttons(&editor.cos, &mut form);
        let display = display_bytes(Some(editor), &bytes);
        let mut config = doc.config.clone();
        config.password = editor.keys.render.as_deref().map(Arc::from);
        let renderer = RenderPool::new(display.clone(), render_threads(), config);
        Ok(Self {
            marks: kition_pdf_edit::marks_present(&editor.cos),
            added: kition_pdf_edit::list_added(&editor.cos),
            links: kition_pdf_annot::links::list(&editor.cos),
            signatures: signatures_of(&editor.cos, &bytes, &doc.trust, &doc.sig_cache),
            bytes,
            display,
            info,
            renderer,
            form: Arc::new(form),
            xfa,
        })
    }

    pub(super) fn install(self, doc: &mut Document) {
        doc.bytes = self.bytes;
        doc.display = self.display;
        doc.info = self.info;
        doc.renderer = self.renderer;
        doc.form = self.form;
        doc.marks = self.marks;
        doc.added = self.added;
        doc.links = self.links;
        doc.signatures = self.signatures;
        doc.xfa = self.xfa;
    }
}

struct PreparedSave {
    editor: Editor,
    view: PreparedView,
    edited_cos: Option<kition_pdf_cos::Document>,
    extends: bool,
    generation: u64,
    password: Option<String>,
    render_password: Option<String>,
    config: RenderConfig,
    js_output: js::JsOutput,
    xfa_template: Option<Arc<kition_pdf_xfa::model::Template>>,
    xfa_warnings: Vec<String>,
    name: String,
    path: Option<String>,
}

impl Session {
    /// Flatten visible Fill & Sign marks and save them as one synchronous operation.
    /// Serialization, reopening and view preparation precede `write`. A returned
    /// writer error leaves the live document, history and replica unchanged.
    /// The callback must write these exact bytes atomically. It must not report
    /// an error after replacing its destination; crash durability is its concern.
    /// Successful publication retains the live replica and its existing fallback
    /// behavior if a replica operation fails internally.
    pub fn save_with_flatten(
        &mut self,
        id: DocId,
        full: bool,
        path: Option<String>,
        write: impl FnOnce(&[u8]) -> Result<(), EditError>,
    ) -> Result<Arc<Vec<u8>>, EditError> {
        self.check_save_with_flatten(id, full)?;
        let moment = self.moment();
        let js_off = self.js_off;
        let origin = self.origin();
        let history = UndoScope::of(origin.kind).unwrap_or(UndoScope::Person);
        let doc = self.doc_mut(id)?;
        let prepared = guard(|| Self::prepare_flatten_save(doc, full, path, moment, js_off, history)).map_err(EditError::Reopen)??;
        let bytes = prepared.view.bytes.clone();
        // No live mutation occurs until the caller has accepted the exact bytes.
        write(&bytes)?;
        let PreparedSave {
            editor,
            view,
            edited_cos,
            extends,
            generation,
            password,
            render_password,
            config,
            js_output,
            xfa_template,
            xfa_warnings,
            name,
            path,
        } = prepared;
        // No Result-returning preparation follows the filesystem callback. Keep
        // this original Link: opening a replacement snapshot loses actor history.
        if let Some(cos) = edited_cos {
            // The flattening step clears its own history's redo steps, never the other scope's.
            let keep: Vec<bool> = doc.editor.as_ref().map(|e| e.redo.iter().map(|u| u.4 != history).collect()).unwrap_or_default();
            doc.link.keep_redo(&keep);
            let steps = doc.link.record(&origin, &cos, password.as_deref());
            doc.link.pushed(steps, editor.undo.len());
        }
        doc.link.saved(&editor.cos, extends, password.as_deref());
        doc.editor = Some(editor);
        doc.password = render_password;
        doc.config = config;
        doc.js_output = js_output;
        doc.xfa_template = xfa_template;
        doc.xfa_warnings = xfa_warnings;
        doc.name = name;
        doc.path = path;
        doc.dirty = false;
        doc.generation = generation;
        view.install(doc);
        Ok(bytes)
    }

    fn prepare_flatten_save(
        doc: &Document,
        full: bool,
        path: Option<String>,
        moment: Moment,
        js_off: bool,
        history: UndoScope,
    ) -> Result<PreparedSave, EditError> {
        let mut editor = doc.editor.clone().ok_or_else(|| EditError::ReadOnly(doc.read_only_reason.clone().unwrap_or_default()))?;
        let mut js_output = doc.js_output.clone();
        let mut xfa_template = doc.xfa_template.clone();
        let mut xfa_warnings = doc.xfa_warnings.clone();
        let edited_cos = if let Some(prepared) = Self::prepare_edit(doc, &Edit::FlattenFillSign, moment, js_off)? {
            let previous = std::mem::replace(&mut editor.cos, prepared.cos);
            let previous_keys = std::mem::replace(&mut editor.keys, prepared.keys);
            editor.undo.push((Edit::FlattenFillSign.label(), previous, previous_keys, prepared.scope, history));
            if editor.undo.len() > MAX_UNDO {
                editor.undo.remove(0);
            }
            editor.redo.retain(|u| u.4 != history);
            note_warnings(&mut xfa_warnings, &prepared.xfa_notes);
            if let Some(js) = prepared.cx.js {
                js_output.append(js.output);
            }
            js_output.append(prepared.cx.xfa_out);
            if prepared.cx.xfa_ran_away {
                xfa_template = None;
                js_output.errors.push(XFA_SCRIPTS_OFF.into());
            }
            Some(editor.cos.clone())
        } else {
            None
        };
        if full && doc.is_signed() {
            return Err(EditError::Signed);
        }
        let generation = doc
            .generation
            .checked_add(if edited_cos.is_some() { 2 } else { 1 })
            .ok_or_else(|| EditError::Invalid("the document generation limit was reached".into()))?;
        #[cfg(test)]
        preparation_failure(PreparationStage::Write)?;
        let opts = SaveOptions { mod_date: moment.now.map(kition_pdf_cos::pdf_date), ..SaveOptions::default() };
        let bytes = if full {
            Arc::new(write_full(&editor.cos, &opts).map_err(|error| EditError::Write(error.to_string()))?)
        } else if editor.cos.is_modified() {
            Arc::new(write_incremental(&editor.cos, &opts).map_err(|error| EditError::Write(error.to_string()))?)
        } else {
            editor.cos.bytes().clone()
        };
        let base = editor.cos.bytes();
        let extends = bytes.len() >= base.len() && bytes.starts_with(base);
        #[cfg(test)]
        preparation_failure(PreparationStage::Reopen)?;
        editor.cos = kition_pdf_cos::Document::open_with_password(bytes, editor.keys.reopen.as_deref())
            .map_err(|error| EditError::Reopen(error.to_string()))?;
        editor.keys.protection_pending = false;
        #[cfg(test)]
        preparation_failure(PreparationStage::View)?;
        let view = PreparedView::new(doc, &editor, &xfa_warnings)?;
        let password = editor.keys.reopen.clone().or_else(|| editor.keys.render.clone());
        let render_password = editor.keys.render.clone();
        let mut config = doc.config.clone();
        config.password = render_password.as_deref().map(Arc::from);
        let (name, path) = match path {
            Some(path) => {
                let name = std::path::Path::new(&path).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
                (name, Some(path))
            }
            None => (doc.name.clone(), doc.path.clone()),
        };
        Ok(PreparedSave {
            editor,
            view,
            edited_cos,
            extends,
            generation,
            password,
            render_password,
            config,
            js_output,
            xfa_template,
            xfa_warnings,
            name,
            path,
        })
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
enum PreparationStage {
    Write,
    Reopen,
    View,
}

#[cfg(test)]
std::thread_local! {
    static PREPARATION_FAILURE: std::cell::Cell<Option<PreparationStage>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn preparation_failure(stage: PreparationStage) -> Result<(), EditError> {
    if PREPARATION_FAILURE.get() == Some(stage) { Err(EditError::Invalid(format!("injected preparation failure: {stage:?}"))) } else { Ok(()) }
}

#[cfg(test)]
mod tests;
