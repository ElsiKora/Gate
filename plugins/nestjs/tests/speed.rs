//! The work one call does on a synthetic service, counted rather than timed: every rule of the
//! plugin in one call reads and parses each file once, whatever the rule count, finds what the
//! rules find alone, and fails as a failing rule fails alone. The ignored `measure` prints the time and reads per rule on a tree
//! of about 10 450 files.

mod large;

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;

use gate_typescript::{Allocator, MemorySource, Settings, TypeScript, take_parsed_on_this_thread};
use large::{Counted, RUNNING_RULES, check, rules, tree};

#[test]
fn every_rule_in_one_call_reads_and_parses_each_file_once_and_finds_the_same() {
    let host = Counted::new(tree(4));
    let declared: Vec<String> = gate_sdk::describe(&nestjs::PLUGIN)
        .rules
        .into_iter()
        .map(|rule| rule.name)
        .collect();

    let mut running = Vec::new();
    let mut failing = Vec::new();
    let mut single = BTreeSet::new();
    let mut parsed_alone = BTreeSet::new();
    let mut joined = Vec::new();
    let _ = take_parsed_on_this_thread();
    for name in &declared {
        match check(&host, &[name.as_str()]) {
            Ok(found) => {
                running.push(name.as_str());
                joined.extend(found);
                single.extend(host.take().into_keys());
                parsed_alone.extend(take_parsed_on_this_thread());
            }
            Err(error) => {
                failing.push((name.as_str(), error));
                host.take();
                let _ = take_parsed_on_this_thread();
            }
        }
    }
    // A rule that starts or stops needing options changes what this test compares.
    assert_eq!(
        running.len(),
        RUNNING_RULES,
        "rules that run with their default options: {running:?}; rules that fail: {:?}",
        failing.iter().map(|(name, _)| name).collect::<Vec<_>>()
    );

    let findings = check(&host, &running).expect("the rules run");
    let parsed = take_parsed_on_this_thread();
    let reads = host.take();

    let repeated: Vec<_> = reads.iter().filter(|(_, count)| **count > 1).collect();
    assert_eq!(
        repeated.len(),
        0,
        "{} of {} files read more than once in one call of {} rules, such as {:?}",
        repeated.len(),
        reads.len(),
        running.len(),
        repeated.iter().take(3).collect::<Vec<_>>()
    );
    // The shared reads are the files some rule reads, no more.
    assert_eq!(reads.keys().cloned().collect::<BTreeSet<_>>(), single);
    // The call parses exactly the files the rules parse alone, each once.
    let parsed_once: BTreeSet<String> = parsed.iter().cloned().collect();
    assert_eq!(
        parsed.len(),
        parsed_once.len(),
        "one call of {} rules parsed a file more than once",
        running.len()
    );
    assert_eq!(parsed_once, parsed_alone);
    // Sharing changes no finding: one call reports what the rules report alone, in rule order,
    // the file that does not parse and the one that is not UTF-8 text included.
    assert_eq!(findings, joined);

    // A rule that fails alone fails a call of every rule the same way: the error it fails with
    // alone, of the same kind, named once.
    for (name, error) in &failing {
        let names: Vec<&str> = declared
            .iter()
            .map(String::as_str)
            .filter(|declared| declared == name || running.contains(declared))
            .collect();
        let together = check(&host, &names).expect_err("the rule fails the call");
        host.take();
        let _ = take_parsed_on_this_thread();
        assert_eq!(&together, error, "{name}");
    }
}

/// Time and reads of each rule alone, then of every rule in one call, on about 10 450 files.
/// Run with `cargo test --release -p gate-plugin-nestjs --test speed -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, not a check: it prints, and takes seconds"]
fn measure() {
    let host = Counted::new(tree(183));
    let rules = rules();
    println!("tree: {} files, {} rules", host.len(), rules.len());
    let mut total = 0.0;
    for name in &rules {
        let _ = take_parsed_on_this_thread();
        let started = Instant::now();
        let findings = check(&host, &[name.as_str()]).expect("the rule runs");
        let seconds = started.elapsed().as_secs_f64();
        let reads = host.take();
        total += seconds;
        println!(
            "rule {name:<28} {seconds:>8.3} s  reads {:>6}  files {:>6}  parses {:>6}  findings {:>6}",
            reads.values().sum::<usize>(),
            reads.len(),
            take_parsed_on_this_thread().len(),
            findings.len()
        );
    }
    println!("sum of single-rule calls: {total:.3} s");
    let names: Vec<&str> = rules.iter().map(String::as_str).collect();
    let _ = take_parsed_on_this_thread();
    let started = Instant::now();
    let findings = check(&host, &names).expect("the rules run");
    let reads = host.take();
    println!(
        "one call of every rule: {:.3} s  reads {}  files {}  parses {}  findings {}",
        started.elapsed().as_secs_f64(),
        reads.values().sum::<usize>(),
        reads.len(),
        take_parsed_on_this_thread().len(),
        findings.len()
    );

    // What each rule adds to a call that has already parsed the tree: the call of the rule
    // that reads every file together with the rule, less that rule alone.
    let anchor = "string-enums";
    let alone = |names: &[&str]| {
        let started = Instant::now();
        check(&host, names).expect("the rules run");
        host.take();
        started.elapsed().as_secs_f64()
    };
    let base = alone(&[anchor]);
    println!("anchor {anchor}: {base:.3} s");
    for name in names.iter().filter(|name| **name != anchor) {
        let added = alone(&[anchor, name]) - base;
        println!("added {name:<28} {added:>8.3} s");
    }
}

/// One call of every rule on about 10 450 files and nothing else, so a process that runs only
/// this test shows the call's peak memory (`/usr/bin/time -l`), less the tree's own from
/// [`tree_alone`].
#[test]
#[ignore = "a measurement, not a check: run it alone under a memory meter"]
fn one_call() {
    let host = Counted::new(tree(183));
    let rules = rules();
    let names: Vec<&str> = rules.iter().map(String::as_str).collect();
    let findings = check(&host, &names).expect("the rules run");
    println!("one call: {} findings", findings.len());
}

/// The tree of [`one_call`] without the call: the memory the test itself holds.
#[test]
#[ignore = "a measurement, not a check: run it alone under a memory meter"]
fn tree_alone() {
    let host = Counted::new(tree(183));
    println!("tree alone: {} files", host.len());
}

/// The library's parse of every source of about 10 450 files in one view, as the typescript
/// plugin's parse rule reads them: what parsing costs a rule that needs no semantic model.
#[test]
#[ignore = "a measurement, not a check: it prints, and takes seconds"]
fn parse_alone() {
    let files = tree(183);
    let source = MemorySource::new(&files);
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let paths: Vec<&String> = files.keys().filter(|path| is_source(path)).collect();
    let started = Instant::now();
    let mut fatal = 0;
    for path in &paths {
        let file = typescript.parse(path).expect("the source parses");
        if file.is_some_and(|file| file.fatal()) {
            fatal += 1;
        }
    }
    println!(
        "parse alone: {} sources {:.3} s, {fatal} fatal",
        paths.len(),
        started.elapsed().as_secs_f64()
    );
}

/// A TypeScript or JavaScript source, by its extension.
fn is_source(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "ts" || extension == "js")
}
