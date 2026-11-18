//! Inputs to computed writes. These facts are evaluated in both observed and current files.

use std::cell::RefCell;
use std::collections::BTreeSet;

use kition_pdf_cos::Object;
use serde_json::{Value, json};

use super::{Reader, deep, overlaps, rects_of};

impl Reader {
    pub(super) fn fill_inputs(&self, written: &[String]) -> Value {
        use kition_pdf_forms::{FieldKind, appearance::parse_da, flags};
        let form = self.cos.root().and_then(|r| self.cos.get(r).as_dict()?.get(b"AcroForm").and_then(|o| self.cos.dict(o)));
        let fonts = form.and_then(|af| af.get(b"DR").and_then(|o| self.cos.dict(o))).and_then(|dr| dr.get(b"Font").and_then(|o| self.cos.dict(o)));
        let all = kition_pdf_forms::fields(&self.cos);
        json!(
            written
                .iter()
                .map(|name| {
                    let Some(f) = all.iter().find(|f| &f.name == name) else { return Value::Null };
                    let mask = flags::READ_ONLY
                        | match f.kind {
                            FieldKind::Text => flags::MULTILINE | flags::PASSWORD | flags::COMB,
                            FieldKind::Combo => flags::EDIT,
                            FieldKind::List => flags::MULTI_SELECT,
                            FieldKind::Radio => flags::NO_TOGGLE_TO_OFF,
                            _ => 0,
                        };
                    let button = matches!(f.kind, FieldKind::Radio | FieldKind::CheckBox);
                    let widgets: Vec<_> = f
                        .widgets
                        .iter()
                        .map(|w| {
                            let d = self.cos.get(w.obj).as_dict().cloned().unwrap_or_default();
                            let keys: &[&[u8]] = if button { &[b"Rect", b"MK", b"BS"] } else { &[b"Rect", b"MK", b"BS", b"DA", b"Q", b"TI"] };
                            let settings: Vec<_> = keys.iter().map(|k| d.get(k).map(|o| deep(&self.cos, o))).collect();
                            let da = d.get(b"DA").and_then(|o| self.cos.resolve(o).as_string().map(|s| s.to_text())).unwrap_or_else(|| f.da.clone());
                            let font = fonts.as_ref().and_then(|fonts| fonts.get(parse_da(&da).font.as_bytes())).map(|o| deep(&self.cos, o));
                            json!([settings, font, button.then(|| json!([w.on_state, d.contains(b"AP")]))])
                        })
                        .collect();
                    // NoToggleToOff consults the prior selection only when clearing a radio group.
                    json!([
                        f.name,
                        f.flags & mask,
                        widgets,
                        (f.kind == FieldKind::Radio && f.has(flags::NO_TOGGLE_TO_OFF)).then_some(!f.value.is_empty())
                    ])
                })
                .collect::<Vec<_>>()
        )
    }

    pub(super) fn overlapping_text(&self, number: u64, params: &Value) -> Value {
        let Some(page) = usize::try_from(number).ok().and_then(|p| p.checked_sub(1)) else { return Value::Null };
        let Some(doc) = self.doc.doc() else { return Value::Null };
        let lines = doc.text_lines(page);
        let index = |key: &str| params.get(key).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).and_then(|n| n.checked_sub(1));
        let members: Vec<_> = if let Some(block) = index("paragraph") {
            doc.text_blocks(page).get(block).into_iter().flat_map(|b| b.lines.iter()).filter_map(|i| lines.get(*i).map(|l| l.rect)).collect()
        } else {
            index("line").and_then(|i| lines.get(i)).map(|l| vec![l.rect]).unwrap_or_default()
        };
        // The editor removes coincident copies when over half of the smaller line is covered.
        let area = |r: [f64; 4]| ((r[2] - r[0]) * (r[3] - r[1])).max(0.0);
        let held: Vec<_> = lines
            .iter()
            .filter(|l| {
                members.iter().any(|m| {
                    let small = area(*m).min(area(l.rect));
                    let x = (m[2].min(l.rect[2]) - m[0].max(l.rect[0])).max(0.0);
                    let y = (m[3].min(l.rect[3]) - m[1].max(l.rect[1])).max(0.0);
                    small > 0.0 && x * y / small > 0.5
                })
            })
            .map(|l| json!([l.text, l.rect, l.base_font, l.size, l.color]))
            .collect();
        json!({"page":self.page(number),"lines":held})
    }

    pub(super) fn added_content(&self, page: u64, index: u64) -> Option<String> {
        let page = usize::try_from(page).ok()?.checked_sub(1)?;
        let index = usize::try_from(index).ok()?.checked_sub(1)?;
        let item = self.doc.doc()?.added.iter().filter(|item| item.page == page).nth(index)?;
        Some(deep(&self.cos, &Object::Ref(item.obj)))
    }

    pub(super) fn image_content(&self, page: u64, index: u64) -> Option<String> {
        let page = usize::try_from(page).ok()?.checked_sub(1)?;
        let index = usize::try_from(index).ok()?.checked_sub(1)?;
        let doc = self.doc.doc()?;
        let images = doc.page_images(page);
        let image = images.get(index)?;
        match image.object {
            Some(object) => Some(deep(&self.cos, &Object::Ref(object))),
            None => doc.page_image_file(page, index).ok().map(|(_, bytes)| kition_kit::session::sha256_hex(&bytes)),
        }
    }

    pub(super) fn calculation_inputs(&self, written: &[String]) -> Value {
        use kition_pdf_forms::af::{Calculate, calculate};
        let all = kition_pdf_forms::fields(&self.cos);
        let mut calculated = Vec::new();
        let mut sources = BTreeSet::new();
        let mut arbitrary = false;
        for field in &all {
            let calculates = field.actions.calculate != Calculate::None || field.actions.scripts.calculate.is_some();
            arbitrary |= field.actions.scripts.calculate.is_some()
                || ((calculates || written.contains(&field.name))
                    && (field.actions.scripts.format.is_some()
                        || field.actions.scripts.validate.is_some()
                        || field.actions.scripts.keystroke.is_some()));
            if !calculates {
                continue;
            }
            calculated.push(json!([
                field.name,
                format!("{:?}", field.actions),
                field.value,
                field.widgets.iter().map(|w| deep(&self.cos, &Object::Ref(w.obj))).collect::<Vec<_>>()
            ]));
            // Use the calculation parser itself to discover both simple and notation inputs.
            // The lookup's borrow ends on each call, before the parser can call it again.
            let looked_up = RefCell::new(BTreeSet::new());
            let lookup = |name: &str| {
                let prefix = format!("{name}.");
                let matched: Vec<_> = all.iter().filter(|f| f.name == name || f.name.starts_with(&prefix)).collect();
                if let Ok(mut names) = looked_up.try_borrow_mut() {
                    names.extend(matched.iter().map(|f| f.name.clone()));
                }
                matched.iter().filter_map(|f| f.value.first().cloned()).collect()
            };
            let _ = calculate(&field.actions.calculate, &lookup);
            sources.extend(looked_up.into_inner());
        }
        let values: Vec<_> = all
            .iter()
            .filter(|f| arbitrary || (sources.contains(&f.name) && !written.contains(&f.name)))
            .map(|f| json!([f.name, f.value, if arbitrary { Some(format!("{:?}", f.actions)) } else { None }]))
            .collect();
        let order = self
            .cos
            .root()
            .and_then(|r| self.cos.get(r).as_dict()?.get(b"AcroForm").and_then(|af| self.cos.dict(af)))
            .and_then(|af| af.get(b"CO").map(|o| self.cos.resolve(o)).and_then(|o| o.as_array().cloned()))
            .unwrap_or_default()
            .iter()
            .filter_map(Object::as_ref)
            .filter_map(|r| all.iter().find(|f| f.obj == r).map(|f| f.name.clone()))
            .collect::<Vec<_>>();
        json!({"calculated": calculated, "sources": values, "order": order,
            "scriptEnvironment": arbitrary.then(|| json!([self.pages.len(), self.cos.trailer().get(b"Info").map(|o| deep(&self.cos,o))]))})
    }

    pub(super) fn heading_inputs(&self) -> Value {
        json!(
            kition_pdf_engine::a11y::headings(&self.cos)
                .iter()
                .map(|heading| json!([heading.level, heading.title, self.page(heading.page as u64 + 1)]))
                .collect::<Vec<_>>()
        )
    }

    pub(super) fn flatten_inputs(&self, comments: bool, fields: bool) -> Result<Value, String> {
        let mut out = Vec::new();
        for (index, _) in self.pages.iter().enumerate() {
            let number = index as u64 + 1;
            let Some(page) = self.page_dict(number) else { continue };
            let annots = page.get(b"Annots").map(|o| self.cos.resolve(o));
            let chosen: Vec<_> = annots
                .as_deref()
                .and_then(Object::as_array)
                .into_iter()
                .flatten()
                .filter(|o| {
                    self.cos.dict(o).is_some_and(|d| {
                        let subtype = d.name(b"Subtype").unwrap_or_default();
                        !matches!(subtype, b"Link" | b"Popup") && if subtype == b"Widget" { fields } else { comments }
                    })
                })
                .map(|o| {
                    let d = self.cos.dict(o).unwrap_or_default();
                    let flags = d.get(b"F").and_then(|o| self.cos.resolve(o).as_int()).unwrap_or(0);
                    let hidden = flags & (2 | 32) != 0;
                    if hidden {
                        return Ok(json!({"hidden":true}));
                    }
                    let normal = d.get(b"AP").and_then(|o| self.cos.dict(o)).and_then(|ap| ap.get(b"N").cloned());
                    let stream = normal.and_then(|normal| {
                        let r = match normal {
                            Object::Ref(r) if matches!(&*self.cos.get(r), Object::Stream(_)) => Some(r),
                            states => self.cos.dict(&states)?.get(d.name(b"AS")?)?.as_ref(),
                        }?;
                        matches!(&*self.cos.get(r), Object::Stream(_)).then_some(Object::Ref(r))
                    });
                    // Flatten uses a retained AP verbatim. Missing non-widget appearances are
                    // generated by this same builder; note icons do not consume Contents.
                    let stream = stream.or_else(|| {
                        (o.as_ref().is_some() && d.name(b"Subtype") != Some(b"Widget"))
                            .then(|| kition_pdf_engine::annot_text::build(&d).map(Object::Stream))
                            .flatten()
                    });
                    let rect = d
                        .get(b"Rect")
                        .map(|o| self.cos.resolve(o))
                        .and_then(|o| o.as_array().and_then(|a| a.iter().map(|o| self.cos.resolve(o).as_f64()).collect::<Option<Vec<_>>>()));
                    let placement = stream.as_ref().zip(rect).and_then(|(stream, rect)| {
                        let [a, b, c, d] = rect.as_slice() else { return None };
                        let form = self.cos.resolve(stream).as_dict().cloned()?;
                        kition_pdf_engine::appearance_placement(&self.cos, &form, [a.min(*c), b.min(*d), a.max(*c), b.max(*d)])
                    });
                    let appearance = stream.map(|o| painted(&self.cos, &o, 0, &mut BTreeSet::new())).transpose()?;
                    Ok(json!({"type":d.name(b"Subtype"),"placement":placement, "appearance":appearance}))
                })
                .collect::<Result<_, String>>()?;
            if !chosen.is_empty() {
                out.push(json!([self.page(number), chosen]));
            }
        }
        Ok(json!(out))
    }

    pub(super) fn redaction_inputs(&mut self, pages: &[u64]) -> Result<Value, String> {
        let marks = kition_pdf_redact::marks(&self.cos);
        let mut out = Vec::new();
        for mark in marks.iter().filter(|m| pages.is_empty() || pages.contains(&(m.page as u64 + 1))) {
            let number = mark.page as u64 + 1;
            let rects: Vec<Value> = {
                let Some(info) = self.doc.doc().and_then(|d| d.info.pages.get(mark.page)) else { continue };
                mark.rects
                    .iter()
                    .map(|r| {
                        let a = info.user_to_view(r[0] as f32, r[1] as f32);
                        let b = info.user_to_view(r[2] as f32, r[3] as f32);
                        json!([a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])])
                    })
                    .collect()
            };
            let covered = self.fact(&json!(["covered", number, rects]))?;
            let geometry = self.fact(&json!(["geometry", number]))?;
            let annotations: Vec<_> = self
                .page_dict(number)
                .and_then(|d| d.get(b"Annots").map(|o| self.cos.resolve(o)))
                .as_deref()
                .and_then(Object::as_array)
                .into_iter()
                .flatten()
                .filter(|o| {
                    self.cos.dict(o).is_some_and(|d| {
                        let rect = d.get(b"Rect").map(|o| self.cos.resolve(o));
                        rect.as_deref()
                            .and_then(|r| rects_of(&cos_json(r)).first().copied())
                            .is_some_and(|r| mark.rects.iter().any(|m| overlaps(m, &r)))
                    })
                })
                .map(|o| deep(&self.cos, o))
                .collect();
            out.push(json!([deep(&self.cos, &Object::Ref(mark.obj)), geometry, covered, annotations]));
        }
        Ok(json!(out))
    }
}

fn painted(cos: &kition_pdf_cos::Document, object: &Object, depth: u8, seen: &mut BTreeSet<u32>) -> Result<Value, String> {
    if depth >= 16 {
        return Err("appearance nesting exceeds the comparison limit".into());
    }
    match object {
        Object::Int(n) => Ok(json!(n.to_string())),
        Object::Real(n) => Ok(json!(if *n == 0.0 { "0".to_owned() } else { n.to_string() })),
        Object::Ref(r) => {
            if !seen.insert(r.num) {
                return Err("appearance contains a cyclic resource".into());
            }
            let value = painted(cos, &cos.get(*r), depth + 1, seen);
            seen.remove(&r.num);
            value
        }
        Object::Array(items) => Ok(json!(items.iter().map(|o| painted(cos, o, depth + 1, seen)).collect::<Result<Vec<_>, _>>()?)),
        Object::Dict(d) => Ok(Value::Object(
            d.iter()
                .filter(|(k, _)| !matches!(k.as_slice(), b"Parent" | b"P"))
                .map(|(k, v)| Ok((String::from_utf8_lossy(k).into_owned(), painted(cos, v, depth + 1, seen)?)))
                .collect::<Result<_, String>>()?,
        )),
        Object::Stream(s) => {
            let mut d = s.dict.clone();
            for key in [b"Length".as_slice(), b"Filter", b"DecodeParms"] {
                d.remove(key);
            }
            let content = s.decoded().map(|bytes| kition_kit::session::sha256_hex(&bytes)).map_err(|e| e.to_string())?;
            Ok(json!([painted(cos, &Object::Dict(d), depth + 1, seen)?, content]))
        }
        other => Ok(json!(format!("{other:?}"))),
    }
}

fn cos_json(object: &Object) -> Value {
    match object {
        Object::Int(n) => json!(n),
        Object::Real(n) => json!(n),
        Object::Array(a) => json!(a.iter().map(cos_json).collect::<Vec<_>>()),
        _ => Value::Null,
    }
}
