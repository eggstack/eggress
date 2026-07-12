use eggress_testkit::oracle::schema::{load_scenario_file, validate_scenario_file};
use std::path::Path;

#[test]
fn all_scenario_files_are_valid() {
    let scenarios_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/scenarios");
    if !scenarios_dir.exists() {
        eprintln!("scenarios dir not found, skipping");
        return;
    }

    let entries: Vec<_> = std::fs::read_dir(&scenarios_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
        .collect();

    assert!(!entries.is_empty(), "no scenario TOML files found");

    let mut all_ids = Vec::new();
    for entry in &entries {
        let file = tokio_test::block_on(load_scenario_file(&entry.path()))
            .unwrap_or_else(|e| panic!("failed to load {}: {e}", entry.path().display()));

        let errors = validate_scenario_file(&file);
        assert!(
            errors.is_empty(),
            "validation errors in {}: {:?}",
            entry.path().display(),
            errors
        );

        for scenario in &file.scenarios {
            assert!(
                !all_ids.contains(&scenario.id),
                "duplicate ID: {}",
                scenario.id
            );
            all_ids.push(scenario.id.clone());
        }
    }

    eprintln!(
        "loaded {} scenario files with {} total scenarios",
        entries.len(),
        all_ids.len()
    );
}

#[test]
fn scenario_ids_are_deterministic() {
    let scenarios_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/scenarios");
    if !scenarios_dir.exists() {
        return;
    }

    let first_load: Vec<String> = load_all_scenario_ids(&scenarios_dir);
    let second_load: Vec<String> = load_all_scenario_ids(&scenarios_dir);
    assert_eq!(first_load, second_load);
}

fn load_all_scenario_ids(dir: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    let entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
        .collect();
    for entry in &entries {
        let file = tokio_test::block_on(load_scenario_file(&entry.path())).unwrap();
        for s in &file.scenarios {
            ids.push(s.id.clone());
        }
    }
    ids.sort();
    ids
}
