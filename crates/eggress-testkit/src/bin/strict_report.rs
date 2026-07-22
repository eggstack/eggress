use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use eggress_testkit::strict_manifest::{
    find_strict_manifest_path, validate_strict_manifest_file, StrictManifest,
};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Mode {
    Write,
    Check,
    Json,
}

fn parse_args() -> (Mode, Option<PathBuf>) {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut mode = Mode::Write;
    let mut manifest_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--check" => mode = Mode::Check,
            "--write" => mode = Mode::Write,
            "--json" => mode = Mode::Json,
            "--manifest" => {
                i += 1;
                if i < args.len() {
                    manifest_path = Some(PathBuf::from(&args[i]));
                } else {
                    eprintln!("error: --manifest requires a path argument");
                    std::process::exit(2);
                }
            }
            "--help" | "-h" => {
                eprintln!("Usage: strict-report [--check|--write|--json] [--manifest <path>]");
                eprintln!();
                eprintln!("Modes:");
                eprintln!("  --write   (default) Regenerate and write the strict report");
                eprintln!(
                    "  --check   Regenerate, compare to checked-in report, exit 1 if different"
                );
                eprintln!("  --json    Write machine-readable JSON to stdout");
                std::process::exit(0);
            }
            other => {
                eprintln!("error: unknown argument: {other}");
                eprintln!("Try --help for usage.");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    (mode, manifest_path)
}

// ---------------------------------------------------------------------------
// Provenance helpers
// ---------------------------------------------------------------------------

fn git_head_sha() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    let mut hex_str = String::with_capacity(64);
    for byte in &hash {
        hex_str.push_str(&format!("{:02x}", byte));
    }
    hex_str
}

// ---------------------------------------------------------------------------
// Minimal SHA-256 (no external crate dependency)
// ---------------------------------------------------------------------------

struct Sha256 {
    state: [u32; 8],
    total_len: u64,
    buf: [u8; 64],
    buf_len: usize,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            total_len: 0,
            buf: [0u8; 64],
            buf_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        let mut i = 0;
        self.total_len += data.len() as u64;

        if self.buf_len > 0 {
            while i < data.len() && self.buf_len < 64 {
                self.buf[self.buf_len] = data[i];
                self.buf_len += 1;
                i += 1;
            }
            if self.buf_len == 64 {
                let block = self.buf;
                self.compress(&block);
                self.buf_len = 0;
            }
        }

        while i + 64 <= data.len() {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[i..i + 64]);
            self.compress(&block);
            i += 64;
        }

        while i < data.len() {
            self.buf[self.buf_len] = data[i];
            self.buf_len += 1;
            i += 1;
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_len * 8;

        // Pad
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;

        if self.buf_len > 56 {
            while self.buf_len < 64 {
                self.buf[self.buf_len] = 0;
                self.buf_len += 1;
            }
            let block = self.buf;
            self.compress(&block);
            self.buf_len = 0;
            self.buf = [0u8; 64];
        }

        while self.buf_len < 56 {
            self.buf[self.buf_len] = 0;
            self.buf_len += 1;
        }

        // Append length in bits as big-endian u64
        self.buf[56] = (bit_len >> 56) as u8;
        self.buf[57] = (bit_len >> 48) as u8;
        self.buf[58] = (bit_len >> 40) as u8;
        self.buf[59] = (bit_len >> 32) as u8;
        self.buf[60] = (bit_len >> 24) as u8;
        self.buf[61] = (bit_len >> 16) as u8;
        self.buf[62] = (bit_len >> 8) as u8;
        self.buf[63] = bit_len as u8;

        let block = self.buf;
        self.compress(&block);

        let mut result = [0u8; 32];
        for (i, &word) in self.state.iter().enumerate() {
            result[i * 4] = (word >> 24) as u8;
            result[i * 4 + 1] = (word >> 16) as u8;
            result[i * 4 + 2] = (word >> 8) as u8;
            result[i * 4 + 3] = word as u8;
        }
        result
    }

    fn compress(&mut self, block: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];

        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

fn deterministic_timestamp() -> String {
    // Normalized to UTC midnight for determinism across runs.
    let output = Command::new("date")
        .args(["-u", "+%Y-%m-%dT00:00:00Z"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        });
    output
        .unwrap_or_else(|| "unknown".to_string())
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// Evidence level classification
// ---------------------------------------------------------------------------

fn evidence_level_group(comparator: &str) -> &'static str {
    match comparator {
        "protocol_wire" | "failure_class" | "composition_validity" | "composition_rejection" => {
            "protocol_wire / failure_class / composition_validity"
        }
        "cipher_kat" | "cipher_roundtrip" => "cipher_kat / cipher_roundtrip",
        "cli_flag_parse" | "cli_flag_rejection" => "cli_flag_parse / cli_flag_rejection",
        "process_lifecycle" => "process_lifecycle",
        "module_existence" | "constant_value" => "module_existence / constant_value",
        "async_callable_signature"
        | "enum_membership"
        | "method_signature"
        | "property_existence"
        | "class_hierarchy" => "other_structural",
        _ => "unknown",
    }
}

fn is_structural_only(comparator: &str) -> bool {
    matches!(
        comparator,
        "module_existence"
            | "constant_value"
            | "async_callable_signature"
            | "enum_membership"
            | "method_signature"
            | "property_existence"
            | "class_hierarchy"
    )
}

// ---------------------------------------------------------------------------
// Report data model
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct ReportData {
    total: usize,
    by_status: HashMap<String, usize>,
    by_category: HashMap<String, usize>,
    by_owner: HashMap<String, usize>,
    by_milestone: HashMap<String, usize>,
    by_evidence_level: HashMap<String, usize>,
    gap_records: Vec<GapEntry>,
    needs_behavioral: Vec<BehavioralEntry>,
    terminal_records: Vec<TerminalEntry>,
}

#[derive(Debug)]
struct GapEntry {
    id: String,
    status: String,
    category: String,
    owner: String,
    milestone: String,
}

#[derive(Debug)]
struct BehavioralEntry {
    id: String,
    comparator: String,
    category: String,
    notes: String,
}

#[derive(Debug)]
struct TerminalEntry {
    id: String,
    status: String,
    category: String,
    #[allow(dead_code)]
    comparator: String,
    notes: String,
}

fn build_report_data(manifest: &StrictManifest) -> ReportData {
    let mut data = ReportData {
        total: manifest.record.len(),
        ..Default::default()
    };

    const TERMINAL: &[&str] = &[
        "drop_in",
        "not_applicable",
        "known_upstream_defect",
        "platform_constraint",
        "intentional_non_parity",
        "structural",
    ];

    for rec in &manifest.record {
        *data.by_status.entry(rec.status.clone()).or_default() += 1;
        *data.by_category.entry(rec.category.clone()).or_default() += 1;
        if !rec.owner.is_empty() {
            *data.by_owner.entry(rec.owner.clone()).or_default() += 1;
        }
        if !rec.milestone.is_empty() {
            *data.by_milestone.entry(rec.milestone.clone()).or_default() += 1;
        }

        let el = evidence_level_group(&rec.comparator);
        *data.by_evidence_level.entry(el.to_string()).or_default() += 1;

        if !TERMINAL.contains(&rec.status.as_str()) {
            data.gap_records.push(GapEntry {
                id: rec.id.clone(),
                status: rec.status.clone(),
                category: rec.category.clone(),
                owner: rec.owner.clone(),
                milestone: rec.milestone.clone(),
            });
        }

        if is_structural_only(&rec.comparator) {
            data.needs_behavioral.push(BehavioralEntry {
                id: rec.id.clone(),
                comparator: rec.comparator.clone(),
                category: rec.category.clone(),
                notes: rec.notes.clone(),
            });
        }

        if TERMINAL.contains(&rec.status.as_str()) {
            data.terminal_records.push(TerminalEntry {
                id: rec.id.clone(),
                status: rec.status.clone(),
                category: rec.category.clone(),
                comparator: rec.comparator.clone(),
                notes: rec.notes.clone(),
            });
        }
    }

    data
}

// ---------------------------------------------------------------------------
// Markdown report generation
// ---------------------------------------------------------------------------

fn format_markdown(
    manifest: &StrictManifest,
    data: &ReportData,
    sha: &str,
    manifest_hash: &str,
    timestamp: &str,
) -> String {
    let mut out = String::with_capacity(8192);

    out.push_str("# pproxy 2.7.9 Strict Compatibility Report\n\n");

    out.push_str("> **CORRECTIVE PASS NOTICE:** This report was regenerated as part of the\n");
    out.push_str("> Milestones A–C Corrective Pass (`plans/MILESTONES_A_C_CORRECTIVE_PASS.md`).\n");
    out.push_str(
        "> Records using `module_existence` comparators with `drop_in` status have namespace\n",
    );
    out.push_str(
        "> evidence only and require behavioral validation before true drop_in status can be\n",
    );
    out.push_str("> claimed. See the corrective pass plan for details.\n\n");

    out.push_str(&format!(
        "**Oracle version:** pproxy=={}\n",
        manifest.meta.pproxy_version
    ));
    out.push_str(&format!("**Manifest schema:** {}\n", manifest.meta.schema));
    out.push_str(&format!("**Policy:** {}\n", manifest.meta.policy_ref));
    out.push_str(&format!("**Oracle ref:** {}\n", manifest.meta.oracle_ref));
    out.push_str(&format!("**Commit SHA:** `{sha}`\n"));
    out.push_str(&format!("**Manifest hash:** `{manifest_hash}`\n"));
    out.push_str(&format!("**Generated:** {timestamp}\n\n"));

    // Summary
    let terminal = data.total - data.gap_records.len();
    let behavioral_count: usize = data
        .by_evidence_level
        .iter()
        .filter(|(k, _)| {
            !k.contains("module_existence")
                && !k.contains("constant_value")
                && !k.contains("other_structural")
        })
        .map(|(_, v)| v)
        .sum();
    let readiness = if data.total > 0 {
        (behavioral_count as f64 / data.total as f64) * 100.0
    } else {
        0.0
    };

    out.push_str("## Summary\n\n");
    out.push_str("| Metric | Count |\n");
    out.push_str("|--------|-------|\n");
    out.push_str(&format!("| Total records | {} |\n", data.total));
    out.push_str(&format!("| Terminal (resolved) | {terminal} |\n"));
    out.push_str(&format!(
        "| Gap (unresolved) | {} |\n",
        data.gap_records.len()
    ));
    out.push_str(&format!(
        "| Needs behavioral evidence | {} |\n",
        data.needs_behavioral.len()
    ));
    out.push_str(&format!(
        "| Certification readiness | {readiness:.0}% |\n\n"
    ));

    // By Status
    let mut status_sorted: Vec<_> = data.by_status.iter().collect();
    status_sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    out.push_str("### By Status\n\n");
    out.push_str("| Status | Count | Notes |\n");
    out.push_str("|--------|-------|-------|\n");
    for (status, count) in &status_sorted {
        let notes = match status.as_str() {
            "drop_in" => {
                let drop_in_structural: usize = data
                    .needs_behavioral
                    .iter()
                    .filter(|e| {
                        manifest
                            .record
                            .iter()
                            .any(|r| r.id == e.id && r.status == "drop_in")
                    })
                    .count();
                if drop_in_structural > 0 {
                    format!("{} need behavioral evidence upgrade", drop_in_structural)
                } else {
                    String::new()
                }
            }
            "gap" => {
                let gap_ids: Vec<_> = data.gap_records.iter().map(|g| g.id.as_str()).collect();
                format!("Gap — {}", gap_ids.join(", "))
            }
            "platform_constraint" => {
                let ids: Vec<_> = manifest
                    .record
                    .iter()
                    .filter(|r| r.status == "platform_constraint")
                    .map(|r| r.id.rsplit('.').next().unwrap_or(&r.id).to_string())
                    .collect();
                ids.join(", ")
            }
            "not_applicable" => "Internal details, daemon, Rule".to_string(),
            "intentional_non_parity" => "SSH, SSR".to_string(),
            _ => String::new(),
        };
        out.push_str(&format!("| {status} | {count} | {notes} |\n"));
    }
    out.push('\n');

    // By Evidence Level
    let mut ev_sorted: Vec<_> = data.by_evidence_level.iter().collect();
    ev_sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    out.push_str("### By Evidence Level\n\n");
    out.push_str("| Evidence Level | Count | Notes |\n");
    out.push_str("|----------------|-------|-------|\n");
    for (level, count) in &ev_sorted {
        let notes = if level.contains("module_existence") || level.contains("constant_value") {
            "**Namespace evidence only — needs behavioral validation**"
        } else if level.contains("protocol_wire") || level.contains("failure_class") {
            "True behavioral evidence"
        } else if level.contains("cipher") {
            "Cipher behavioral evidence"
        } else if level.contains("cli_flag") {
            "CLI parsing evidence"
        } else if level.contains("process") {
            "Process lifecycle evidence"
        } else {
            ""
        };
        out.push_str(&format!("| {level} | {count} | {notes} |\n"));
    }
    out.push('\n');

    // By Category
    let mut cat_sorted: Vec<_> = data.by_category.iter().collect();
    cat_sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    out.push_str("### By Category\n\n");
    out.push_str("| Category | Count |\n");
    out.push_str("|----------|-------|\n");
    for (cat, count) in &cat_sorted {
        out.push_str(&format!("| {cat} | {count} |\n"));
    }
    out.push('\n');

    // By Owner
    let mut owner_sorted: Vec<_> = data.by_owner.iter().collect();
    owner_sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    out.push_str("### By Owner\n\n");
    out.push_str("| Owner | Count |\n");
    out.push_str("|-------|-------|\n");
    for (owner, count) in &owner_sorted {
        out.push_str(&format!("| {owner} | {count} |\n"));
    }
    out.push('\n');

    // By Milestone
    let mut ms_sorted: Vec<_> = data.by_milestone.iter().collect();
    ms_sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    out.push_str("### By Milestone\n\n");
    out.push_str("| Milestone | Count |\n");
    out.push_str("|-----------|-------|\n");
    for (ms, count) in &ms_sorted {
        out.push_str(&format!("| {ms} | {count} |\n"));
    }
    out.push('\n');

    // Gap Records
    out.push_str("## Gap Records\n\n");
    if data.gap_records.is_empty() {
        out.push_str("_No unresolved gaps._\n\n");
    } else {
        out.push_str(&format!(
            "Records with unresolved `gap` status ({} total):\n\n",
            data.gap_records.len()
        ));
        out.push_str("| ID | Status | Category | Owner | Milestone |\n");
        out.push_str("|----|--------|----------|-------|----------|\n");
        for gap in &data.gap_records {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                gap.id, gap.status, gap.category, gap.owner, gap.milestone
            ));
        }
        out.push('\n');
    }

    // Records Needing Behavioral Evidence
    out.push_str("## Records Needing Behavioral Evidence\n\n");
    if data.needs_behavioral.is_empty() {
        out.push_str("_All records have behavioral evidence._\n\n");
    } else {
        out.push_str(&format!(
            "The following {} records use structural comparators (module_existence, method_signature,\n",
            data.needs_behavioral.len()
        ));
        out.push_str(
            "constant_value, property_existence, class_hierarchy) and have namespace-level evidence\n",
        );
        out.push_str(
            "only. These require paired oracle/candidate behavioral validation (protocol_wire,\n",
        );
        out.push_str(
            "cipher_kat, cipher_roundtrip, etc.) before their status can be upgraded to `drop_in`.\n\n",
        );

        // Group by category
        let mut by_cat: HashMap<String, Vec<&BehavioralEntry>> = HashMap::new();
        for entry in &data.needs_behavioral {
            by_cat
                .entry(entry.category.clone())
                .or_default()
                .push(entry);
        }

        let mut cat_keys: Vec<_> = by_cat.keys().collect();
        cat_keys.sort();
        for cat in &cat_keys {
            let entries = &by_cat[*cat];
            out.push_str(&format!("### {} ({} records)\n\n", cat, entries.len()));
            out.push_str("| ID | Comparator | Notes |\n");
            out.push_str("|----|-----------|-------|\n");
            for entry in entries {
                let notes_short = if entry.notes.is_empty() {
                    "-".to_string()
                } else if entry.notes.len() > 60 {
                    format!("{}...", &entry.notes[..57])
                } else {
                    entry.notes.clone()
                };
                out.push_str(&format!(
                    "| {} | {} | {} |\n",
                    entry.id, entry.comparator, notes_short
                ));
            }
            out.push('\n');
        }
    }

    // Terminal Records
    out.push_str("## Terminal Records\n\n");
    let terminal_count = data.terminal_records.len();
    out.push_str(&format!(
        "### {terminal_count} records with terminal status\n\n"
    ));

    // Group terminal records by category
    let mut term_by_cat: HashMap<String, Vec<&TerminalEntry>> = HashMap::new();
    for entry in &data.terminal_records {
        term_by_cat
            .entry(entry.category.clone())
            .or_default()
            .push(entry);
    }

    let mut term_cats: Vec<_> = term_by_cat.keys().collect();
    term_cats.sort();
    for cat in &term_cats {
        let entries = &term_by_cat[*cat];
        out.push_str(&format!("### {} ({} records)\n\n", cat, entries.len()));
        out.push_str("| ID | Status | Notes |\n");
        out.push_str("|----|--------|-------|\n");
        for entry in entries {
            let notes_short = if entry.notes.is_empty() {
                "-".to_string()
            } else if entry.notes.len() > 80 {
                format!("{}...", &entry.notes[..77])
            } else {
                entry.notes.clone()
            };
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                entry.id, entry.status, notes_short
            ));
        }
        out.push('\n');
    }

    out
}

// ---------------------------------------------------------------------------
// JSON report generation
// ---------------------------------------------------------------------------

fn format_json(
    manifest: &StrictManifest,
    data: &ReportData,
    sha: &str,
    manifest_hash: &str,
    timestamp: &str,
) -> String {
    let mut out = String::with_capacity(8192);

    out.push_str("{\n");
    out.push_str(&format!(
        "  \"oracle_version\": \"{}\",\n",
        manifest.meta.pproxy_version
    ));
    out.push_str(&format!(
        "  \"manifest_schema\": \"{}\",\n",
        manifest.meta.schema
    ));
    out.push_str(&format!("  \"commit_sha\": \"{sha}\",\n"));
    out.push_str(&format!("  \"manifest_hash\": \"{manifest_hash}\",\n"));
    out.push_str(&format!("  \"generated\": \"{timestamp}\",\n"));

    let terminal = data.total - data.gap_records.len();
    let behavioral_count: usize = data
        .by_evidence_level
        .iter()
        .filter(|(k, _)| {
            !k.contains("module_existence")
                && !k.contains("constant_value")
                && !k.contains("other_structural")
        })
        .map(|(_, v)| v)
        .sum();
    let readiness = if data.total > 0 {
        (behavioral_count as f64 / data.total as f64) * 100.0
    } else {
        0.0
    };

    out.push_str("  \"summary\": {\n");
    out.push_str(&format!("    \"total\": {},\n", data.total));
    out.push_str(&format!("    \"terminal\": {terminal},\n"));
    out.push_str(&format!("    \"gaps\": {},\n", data.gap_records.len()));
    out.push_str(&format!(
        "    \"needs_behavioral\": {},\n",
        data.needs_behavioral.len()
    ));
    out.push_str(&format!("    \"readiness_pct\": {readiness:.1}\n"));
    out.push_str("  },\n");

    // by_status
    out.push_str("  \"by_status\": {\n");
    let mut status_sorted: Vec<_> = data.by_status.iter().collect();
    status_sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (i, (status, count)) in status_sorted.iter().enumerate() {
        let comma = if i + 1 < status_sorted.len() { "," } else { "" };
        out.push_str(&format!("    \"{status}\": {count}{comma}\n"));
    }
    out.push_str("  },\n");

    // by_category
    out.push_str("  \"by_category\": {\n");
    let mut cat_sorted: Vec<_> = data.by_category.iter().collect();
    cat_sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (i, (cat, count)) in cat_sorted.iter().enumerate() {
        let comma = if i + 1 < cat_sorted.len() { "," } else { "" };
        out.push_str(&format!("    \"{cat}\": {count}{comma}\n"));
    }
    out.push_str("  },\n");

    // by_evidence_level
    out.push_str("  \"by_evidence_level\": {\n");
    let mut ev_sorted: Vec<_> = data.by_evidence_level.iter().collect();
    ev_sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (i, (level, count)) in ev_sorted.iter().enumerate() {
        let comma = if i + 1 < ev_sorted.len() { "," } else { "" };
        out.push_str(&format!("    \"{level}\": {count}{comma}\n"));
    }
    out.push_str("  },\n");

    // gaps
    out.push_str("  \"gaps\": [\n");
    for (i, gap) in data.gap_records.iter().enumerate() {
        let comma = if i + 1 < data.gap_records.len() {
            ","
        } else {
            ""
        };
        out.push_str(&format!(
            "    {{\"id\": \"{}\", \"status\": \"{}\", \"category\": \"{}\", \"owner\": \"{}\", \"milestone\": \"{}\"}}{comma}\n",
            gap.id, gap.status, gap.category, gap.owner, gap.milestone
        ));
    }
    out.push_str("  ],\n");

    // needs_behavioral
    out.push_str("  \"needs_behavioral\": [\n");
    for (i, entry) in data.needs_behavioral.iter().enumerate() {
        let comma = if i + 1 < data.needs_behavioral.len() {
            ","
        } else {
            ""
        };
        out.push_str(&format!(
            "    {{\"id\": \"{}\", \"comparator\": \"{}\", \"category\": \"{}\"}}{comma}\n",
            entry.id, entry.comparator, entry.category
        ));
    }
    out.push_str("  ]\n");

    out.push_str("}\n");
    out
}

// ---------------------------------------------------------------------------
// Timestamp normalization for --check
// ---------------------------------------------------------------------------

fn normalize_timestamps(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    for line in content.lines() {
        if line.starts_with("**Generated:**") || line.starts_with("  \"generated\":") {
            // Replace with normalized placeholder
            if line.contains('"') {
                result.push_str("  \"generated\": \"<normalized>\",\n");
            } else {
                result.push_str("**Generated:** <normalized>\n");
            }
        } else if line.starts_with("**Commit SHA:**") || line.starts_with("  \"commit_sha\":") {
            // Replace with normalized placeholder — SHA differs between commits
            if line.contains('"') {
                result.push_str("  \"commit_sha\": \"<normalized>\",\n");
            } else {
                result.push_str("**Commit SHA:** `<normalized>`\n");
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let (mode, manifest_override) = parse_args();

    // Locate the manifest
    let manifest_path = manifest_override
        .or_else(find_strict_manifest_path)
        .unwrap_or_else(|| {
            eprintln!("error: strict manifest not found");
            eprintln!("Use --manifest <path> or run from within the workspace.");
            std::process::exit(1);
        });

    // Parse and validate
    let manifest = validate_strict_manifest_file(&manifest_path).unwrap_or_else(|errs| {
        eprintln!("error: strict manifest validation failed:");
        for err in &errs.errors {
            eprintln!("  - {err}");
        }
        std::process::exit(1);
    });

    // Provenance
    let sha = git_head_sha();
    let manifest_bytes = fs::read(&manifest_path).expect("failed to read manifest for hashing");
    let manifest_hash = sha256_hex(&manifest_bytes);
    let timestamp = deterministic_timestamp();

    // Build data
    let data = build_report_data(&manifest);

    match mode {
        Mode::Json => {
            let json = format_json(&manifest, &data, &sha, &manifest_hash, &timestamp);
            std::io::stdout().write_all(json.as_bytes()).unwrap();
        }
        Mode::Write | Mode::Check => {
            let report = format_markdown(&manifest, &data, &sha, &manifest_hash, &timestamp);

            // Determine output path — manifest lives at docs/parity/, report lives there too
            let output_path = manifest_path
                .parent()
                .unwrap_or(&manifest_path)
                .join("PPROXY_2_7_9_STRICT_REPORT.md");

            if Mode::Check == mode {
                // Read existing report
                let existing = match fs::read_to_string(&output_path) {
                    Ok(c) => c,
                    Err(_) => {
                        eprintln!(
                            "error: cannot read existing report at {}",
                            output_path.display()
                        );
                        std::process::exit(1);
                    }
                };

                let normalized_existing = normalize_timestamps(&existing);
                let normalized_new = normalize_timestamps(&report);

                if normalized_existing == normalized_new {
                    println!("PASS: report is up to date");
                    std::process::exit(0);
                } else {
                    eprintln!("FAIL: report differs from checked-in version");
                    eprintln!("  Regenerated report differs. Run without --check to update.");
                    // Show a compact diff summary
                    let existing_lines: Vec<&str> = normalized_existing.lines().collect();
                    let new_lines: Vec<&str> = normalized_new.lines().collect();
                    let mut diff_count = 0;
                    let max_diffs = 20;
                    for (i, (a, b)) in existing_lines.iter().zip(new_lines.iter()).enumerate() {
                        if a != b {
                            diff_count += 1;
                            if diff_count <= max_diffs {
                                eprintln!("  line {}: - {}", i + 1, a);
                                eprintln!("  line {}: + {}", i + 1, b);
                            }
                        }
                    }
                    if existing_lines.len() != new_lines.len() {
                        diff_count += 1;
                        eprintln!(
                            "  line count differs: existing={}, generated={}",
                            existing_lines.len(),
                            new_lines.len()
                        );
                    }
                    if diff_count > max_diffs {
                        eprintln!("  ... and {} more differences", diff_count - max_diffs);
                    }
                    eprintln!("  total differences: {diff_count}");
                    std::process::exit(1);
                }
            } else {
                // Write mode
                if let Some(parent) = output_path.parent() {
                    fs::create_dir_all(parent).ok();
                }
                fs::write(&output_path, &report).unwrap_or_else(|e| {
                    eprintln!("error: failed to write {}: {e}", output_path.display());
                    std::process::exit(1);
                });
                eprintln!(
                    "Wrote strict report to {} ({} records, {manifest_hash})",
                    output_path.display(),
                    data.total
                );
            }
        }
    }
}
