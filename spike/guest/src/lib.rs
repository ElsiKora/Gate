//! The workload as a `wasm32-wasip2` component. The tree comes through the `tree` host calls, or,
//! with `"access": "fs"`, through the WASI directory the host preopens at `/tree` (the listing still
//! comes from the host, as git lists the tracked files).

use spike_workload::{DirectoryTree, Outcome, Request, Tree};

wit_bindgen::generate!({ world: "plugin", path: "../wit" });

use gate::spike::tree;

/// The tree over the `tree` host calls: one call per listing, one per file read.
struct HostTree;

impl Tree for HostTree {
    fn files(&mut self) -> Vec<String> {
        tree::list_files()
    }

    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        tree::read_file(path)
    }
}

fn with_tree<R>(request: &str, body: impl FnOnce(&mut dyn Tree, Request) -> R) -> R {
    let value: serde_json::Value = serde_json::from_str(request).unwrap_or_default();
    let tasks = value["tasks"]
        .as_array()
        .map(|tasks| tasks.iter().filter_map(|task| task.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    let request = Request { tasks };
    if value["access"].as_str() == Some("fs") {
        let mut tree = DirectoryTree { root: "/tree".into(), listing: tree::list_files() };
        body(&mut tree, request)
    } else {
        body(&mut HostTree, request)
    }
}

struct Plugin;

impl Guest for Plugin {
    fn run(request: String) -> String {
        with_tree(&request, |tree, request| {
            serde_json::to_string(&spike_workload::run(tree, &request)).unwrap_or_default()
        })
    }

    fn table(request: String) -> Vec<u8> {
        with_tree(&request, |tree, _| {
            let mut outcome = Outcome::default();
            spike_workload::table(tree, &mut outcome)
        })
    }

    fn table_rule(table: Vec<u8>) -> String {
        let mut outcome = Outcome::default();
        spike_workload::table_rule(&table, &mut outcome);
        serde_json::to_string(&outcome).unwrap_or_default()
    }
}

export!(Plugin);
