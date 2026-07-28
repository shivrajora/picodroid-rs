// SPDX-License-Identifier: GPL-3.0-only
use std::process;
use std::time::Duration;

use crate::protocol::{recv_response, send_frame, status_str, CMD_SYSMON, STATUS_OK};
use pdb_protocol::sysmon::{state_name, SysmonError, SysmonView};

const BAUD_RATE: u32 = 115_200;
const TIMEOUT: Duration = Duration::from_secs(5);

pub fn run(port_name: &str) {
    let mut port = match serialport::new(port_name, BAUD_RATE)
        .timeout(TIMEOUT)
        .open()
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot open {port_name}: {e}");
            process::exit(1);
        }
    };

    if let Err(e) = send_frame(port.as_mut(), CMD_SYSMON, b"") {
        eprintln!("error: SYSMON send failed: {e}");
        process::exit(1);
    }

    match recv_response(port.as_mut()) {
        Ok((STATUS_OK, payload)) => match SysmonView::parse(&payload) {
            Ok(v) => print_sysmon(&v),
            Err(SysmonError::TooShort(n)) => {
                eprintln!("error: SYSMON response too short ({n} bytes)");
                process::exit(1);
            }
        },
        Ok((status, _)) => {
            eprintln!("error: SYSMON returned {}", status_str(status));
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: SYSMON recv failed: {e}");
            process::exit(1);
        }
    }
}

fn print_sysmon(v: &SysmonView) {
    let uptime_ticks = v.uptime_ticks();
    let free_heap = v.free_heap();
    let min_free_heap = v.min_free_heap();
    let total_run_time = v.total_run_time();
    let task_count = v.task_count();

    let uptime_s = uptime_ticks as f64 / 1000.0;
    println!("Uptime:         {uptime_ticks} ticks ({uptime_s:.1}s)");
    println!(
        "Free heap:      {free_heap} bytes ({:.1} KB)",
        free_heap as f64 / 1024.0
    );
    println!(
        "Min free heap:  {min_free_heap} bytes ({:.1} KB)",
        min_free_heap as f64 / 1024.0
    );
    println!(
        "Total CPU time: {total_run_time} µs ({:.1}s)",
        total_run_time as f64 / 1_000_000.0
    );

    if task_count == 0 {
        return;
    }

    if v.payload_len() < v.expected_len() {
        eprintln!(
            "warning: expected {} bytes for {task_count} tasks, got {}",
            v.expected_len(),
            v.payload_len()
        );
        return;
    }

    println!();
    println!(
        "  {:<16} {:<10} {:>3}  {:>4}  {:>7}  {:>6}",
        "NAME", "STATE", "PRI", "BASE", "STK-HWM", "CPU%"
    );

    for i in 0..task_count {
        let Some(t) = v.task(i) else { break };

        let cpu_str = match t.cpu_permille() {
            None => "N/A".to_string(),
            Some(x) => format!("{:.1}%", x as f64 / 10.0),
        };

        println!(
            "  {:<16} {:<10} {:>3}  {:>4}  {:>5}w  {:>6}",
            t.name(),
            state_name(t.state()),
            t.current_priority(),
            t.base_priority(),
            t.stack_high_water(),
            cpu_str
        );
    }

    // mem-diag firmware appends a 16-byte JVM block after the task entries
    // (jvm_live, post-GC floor, alloc_total, largest free native block).
    // Plain firmware sends exactly expected_len; print the block when
    // present — after the task table, so the rows sit under their header.
    if let Some(d) = v.mem_diag() {
        println!();
        println!("JVM (mem-diag firmware):");
        println!(
            "  Live bytes:     {} ({:.1} KB)",
            d.live,
            d.live as f64 / 1024.0
        );
        println!(
            "  Post-GC floor:  {} bytes ({:.1} KB)",
            d.floor,
            d.floor as f64 / 1024.0
        );
        println!("  Allocs total:   {}", d.alloc_total);
        println!(
            "  Largest free:   {} bytes ({:.1} KB)",
            d.largest_free,
            d.largest_free as f64 / 1024.0
        );
    }
}
