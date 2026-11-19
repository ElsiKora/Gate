//! The workload as a separate process driven over stdin and stdout with [`frame`]s.
//!
//! The host sends `{"op":"run","tasks":[..],"access":"fs"|"rpc","root":".."}`. The plugin asks for
//! the listing (`{"op":"list"}`), and with `"access":"rpc"` asks for every file
//! (`{"op":"read","path":".."}`) instead of reading the directory itself; it answers with
//! `{"op":"result","outcome":..}`. `{"op":"table"}` answers with the flat node table as bytes, and a
//! bytes frame after `{"op":"table-rule"}` is read as such a table.

use std::io::{self, BufReader, BufWriter, Stdin, Stdout};
use std::path::PathBuf;

use spike_workload::frame::{self, BYTES, JSON};
use spike_workload::{DirectoryTree, Outcome, Request, Tree};

struct Pipe {
    input: BufReader<Stdin>,
    output: BufWriter<Stdout>,
}

impl Pipe {
    fn ask(&mut self, message: &serde_json::Value) -> Option<(u8, Vec<u8>)> {
        frame::write(&mut self.output, JSON, message.to_string().as_bytes()).ok()?;
        frame::read(&mut self.input).ok().flatten()
    }
}

/// The tree over the pipe: one round trip per listing and per file.
struct PipeTree<'p>(&'p mut Pipe);

impl Tree for PipeTree<'_> {
    fn files(&mut self) -> Vec<String> {
        listing(self.0)
    }

    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        match self.0.ask(&serde_json::json!({"op": "read", "path": path})) {
            Some((BYTES, bytes)) => Some(bytes),
            _ => None,
        }
    }
}

fn listing(pipe: &mut Pipe) -> Vec<String> {
    pipe.ask(&serde_json::json!({"op": "list"}))
        .and_then(|(_, bytes)| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn with_tree<R>(
    pipe: &mut Pipe,
    request: &serde_json::Value,
    body: impl FnOnce(&mut dyn Tree) -> R,
) -> R {
    if request["access"].as_str() == Some("rpc") {
        body(&mut PipeTree(pipe))
    } else {
        let listing = listing(pipe);
        let root = PathBuf::from(request["root"].as_str().unwrap_or("."));
        body(&mut DirectoryTree { root, listing })
    }
}

fn main() -> io::Result<()> {
    let mut pipe = Pipe {
        input: BufReader::with_capacity(1 << 20, io::stdin()),
        output: BufWriter::with_capacity(1 << 20, io::stdout()),
    };

    while let Some((_, payload)) = frame::read(&mut pipe.input)? {
        let request: serde_json::Value = serde_json::from_slice(&payload).unwrap_or_default();
        match request["op"].as_str() {
            Some("run") => {
                let tasks = serde_json::from_value(request["tasks"].clone()).unwrap_or_default();
                let outcome = with_tree(&mut pipe, &request, |tree| {
                    spike_workload::run(tree, &Request { tasks })
                });
                let answer = serde_json::json!({"op": "result", "outcome": outcome});
                frame::write(&mut pipe.output, JSON, answer.to_string().as_bytes())?;
            }
            Some("table") => {
                let table = with_tree(&mut pipe, &request, |tree| {
                    spike_workload::table(tree, &mut Outcome::default())
                });
                frame::write(&mut pipe.output, BYTES, &table)?;
            }
            Some("table-rule") => {
                let Some((_, table)) = frame::read(&mut pipe.input)? else { break };
                let mut outcome = Outcome::default();
                spike_workload::table_rule(&table, &mut outcome);
                let answer = serde_json::json!({"op": "result", "outcome": outcome});
                frame::write(&mut pipe.output, JSON, answer.to_string().as_bytes())?;
            }
            _ => break,
        }
    }
    Ok(())
}
