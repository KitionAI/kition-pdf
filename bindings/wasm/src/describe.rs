//! What each read command answers (and, where the automation tool's own text needs it, what a
//! command does) (W1-46 P2-1, KHP A1/A5): `commands.json`, one entry per command id, merged into
//! [`KitCommand`]s after they are built. Checked by `kition_kit::testing::check_commands` in
//! `tests/commands.rs`.

use std::collections::HashMap;
use std::sync::OnceLock;

use kition_kit::KitCommand;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Entry {
    /// Replaces the automation tool's own description when given.
    #[serde(default)]
    description: String,
    #[serde(default)]
    output: Option<Value>,
    /// Parameter schema nodes that replace the notation's, by parameter name.
    #[serde(default)]
    params: HashMap<String, Value>,
}

fn table() -> &'static HashMap<String, Entry> {
    static TABLE: OnceLock<HashMap<String, Entry>> = OnceLock::new();
    TABLE.get_or_init(|| serde_json::from_str(include_str!("commands.json")).unwrap_or_default())
}

/// Read commands an agent is offered that answer nothing, and why.
pub const NO_OUTPUT: &[(&str, &str)] = &[];

/// Fills each command's description, output schema, and typed parameters from the table.
pub fn describe(commands: &mut [KitCommand]) {
    let table = table();
    for command in commands {
        let Some(entry) = table.get(&command.id) else { continue };
        if !entry.description.is_empty() {
            command.description.clone_from(&entry.description);
        }
        if entry.output.is_some() {
            command.output_schema.clone_from(&entry.output);
        }
        if let Some(Value::Object(schema)) = command.schema.as_mut() {
            for (name, node) in &entry.params {
                schema.insert(name.clone(), node.clone());
            }
        }
    }
}
