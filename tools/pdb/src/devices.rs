use std::process;

pub fn run() {
    let ports = match serialport::available_ports() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: failed to list serial ports: {e}");
            process::exit(1);
        }
    };

    if ports.is_empty() {
        println!("no devices");
        return;
    }

    for port in &ports {
        println!("{}", port.port_name);
    }
}
