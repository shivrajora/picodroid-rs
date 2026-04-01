use std::process;
use std::time::Duration;

use crate::protocol::{recv_response, send_frame, status_str, CMD_SYSMON, STATUS_OK};

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
        Ok((STATUS_OK, payload)) => {
            if payload.len() < 20 {
                eprintln!("error: SYSMON response too short ({} bytes)", payload.len());
                process::exit(1);
            }
            print_sysmon(&payload);
        }
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

fn print_sysmon(payload: &[u8]) {
    let uptime_ticks = u32::from_le_bytes(payload[0..4].try_into().unwrap());
    let free_heap = u32::from_le_bytes(payload[4..8].try_into().unwrap());
    let min_free_heap = u32::from_le_bytes(payload[8..12].try_into().unwrap());
    let total_run_time = u32::from_le_bytes(payload[12..16].try_into().unwrap());
    let task_count = payload[16] as usize;

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

    let expected_len = 20 + task_count * 28;
    if payload.len() < expected_len {
        eprintln!(
            "warning: expected {} bytes for {task_count} tasks, got {}",
            expected_len,
            payload.len()
        );
        return;
    }

    println!();
    println!(
        "  {:<16} {:<10} {:>3}  {:>4}  {:>7}  {:>6}",
        "NAME", "STATE", "PRI", "BASE", "STK-HWM", "CPU%"
    );

    for i in 0..task_count {
        let base = 20 + i * 28;
        let entry = &payload[base..base + 28];

        let name_bytes = &entry[0..16];
        let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(16);
        let name = std::str::from_utf8(&name_bytes[..name_end]).unwrap_or("?");

        let state = match entry[16] {
            0 => "Running",
            1 => "Ready",
            2 => "Blocked",
            3 => "Suspended",
            4 => "Deleted",
            _ => "Unknown",
        };

        let current_pri = entry[17];
        let base_pri = entry[18];
        let stack_hwm = u16::from_le_bytes([entry[20], entry[21]]);
        let cpu_pct_x10 = u32::from_le_bytes(entry[24..28].try_into().unwrap());

        let cpu_str = if cpu_pct_x10 == 0xFFFF_FFFF {
            "N/A".to_string()
        } else {
            format!("{:.1}%", cpu_pct_x10 as f64 / 10.0)
        };

        println!(
            "  {:<16} {:<10} {:>3}  {:>4}  {:>5}w  {:>6}",
            name, state, current_pri, base_pri, stack_hwm, cpu_str
        );
    }
}
