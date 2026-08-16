// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! File transport contract for core diagnostic records.
//!
//! The core hands over unterminated records; this adapter owns file creation,
//! buffering, and the `\n` terminator that the desktop trace format requires.

use std::fs;
use std::path::PathBuf;

use rumiga_platform::TraceSink;
use rumiga_platform_desktop::FileTraceSink;

/// Test-local path that does not collide with parallel test binaries.
fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("rumiga-{name}-{}.log", std::process::id()))
}

#[test]
fn records_are_written_as_newline_terminated_lines() {
    let path = temp_path("file-trace-sink");
    let mut sink = FileTraceSink::create(&path).expect("sink should be created");

    sink.write_record(format_args!("PC: {:08X}", 0x1000_u32));
    sink.write_record(format_args!("PC: {:08X}", 0x1002_u32));
    sink.flush();

    let written = fs::read_to_string(&path).expect("trace file should exist");
    fs::remove_file(&path).unwrap();

    assert_eq!(written, "PC: 00001000\nPC: 00001002\n");
}

#[test]
fn creating_a_sink_truncates_an_existing_file() {
    let path = temp_path("file-trace-sink-truncate");
    fs::write(&path, "stale content\n").unwrap();

    let mut sink = FileTraceSink::create(&path).expect("sink should be created");
    sink.write_record(format_args!("fresh"));
    sink.flush();

    let written = fs::read_to_string(&path).expect("trace file should exist");
    fs::remove_file(&path).unwrap();

    assert_eq!(written, "fresh\n");
}

#[test]
fn creation_reports_host_filesystem_errors() {
    let path = temp_path("file-trace-sink-missing-dir").join("nested.log");
    assert!(
        FileTraceSink::create(&path).is_err(),
        "creating a sink below a missing directory must fail"
    );
}
