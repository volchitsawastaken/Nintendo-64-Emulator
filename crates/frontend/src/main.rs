//! CLI frontend: library scan, ROM info, step execution.
//! Desktop UI lands at M2+; every command here runs real state.

use n64_common::{EmuConfig, EmuError, Result};
use std::path::PathBuf;

pub struct GameEntry {
    pub path: PathBuf,
    pub title: String,
    pub status: String,
}

pub fn scan_library(cfg: &EmuConfig) -> Result<Vec<GameEntry>> {
    let mut out = Vec::new();
    if !cfg.content_dir.exists() {
        return Ok(out);
    }
    for e in std::fs::read_dir(&cfg.content_dir)? {
        let e = e.map_err(|e| EmuError::Io(e.to_string()))?;
        let p = e.path();
        let title = p
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        match n64_filesystem::identify(&p) {
            Ok(kind) => {
                let status = match kind {
                    n64_filesystem::ContentKind::Rom => "rom (loads, not verified playable)",
                    n64_filesystem::ContentKind::Save => "save",
                    n64_filesystem::ContentKind::Unknown => "unknown (see docs/COMPATIBILITY.md)",
                };
                out.push(GameEntry {
                    path: p,
                    title,
                    status: status.into(),
                });
            }
            Err(err) => out.push(GameEntry {
                path: p,
                title,
                status: format!("identify error: {}", err),
            }),
        }
    }
    Ok(out)
}

pub fn print_help() {
    println!("n64-rs — Nintendo 64 emulator (M1)");
    println!();
    println!("usage:");
    println!("  n64 info <rom>        parse header, print entry/CRC/name");
    println!("  n64 step <rom> <n>     load ROM to RDRAM, step n instructions");
    println!("  n64 scan              list content dir");
}

pub fn cmd_info(rom_path: &str) -> Result<()> {
    let raw = std::fs::read(rom_path)?;
    let rom = n64_loader::parse_rom(&raw)?;
    println!("file: {}", rom_path);
    println!("format: {:?}", rom.header.format);
    println!("name: {}", rom.header.name);
    println!("entry: {:08x}", rom.header.entry);
    println!("clock: {:08x}", rom.header.clock_rate);
    println!("crc: {:08x} {:08x}", rom.header.crc1, rom.header.crc2);
    println!("size: {} bytes", rom.data.len());
    Ok(())
}

pub fn cmd_step(rom_path: &str, n: usize) -> Result<()> {
    use n64_cpu::CpuBackend;
    let raw = std::fs::read(rom_path)?;
    let rom = n64_loader::parse_rom(&raw)?;
    let mut mem = n64_memory::Memory::new();
    let mut cpu = n64_cpu::Interpreter::new(0, 0);
    let boot = n64_kernel::pif::boot(&mut mem, &mut cpu.t, &rom.data, false)?;
    println!(
        "boot mode={:?} pc={:08x} cic={:?}",
        boot.mode, boot.pc, boot.cic
    );
    let mut trace = n64_debugger::TraceBuffer::new(256);
    for _ in 0..n {
        let pc = cpu.thread().pc;
        let word = mem.fetch_u32(pc)?;
        trace.push(pc, word);
        if let Some(trap) = cpu.step(&mut mem)? {
            println!(
                "trap {:?} code={:x} at {:08x}",
                trap.kind, trap.code, trap.pc
            );
            break;
        }
    }
    let t = cpu.thread();
    println!("pc={:08x} steps={} r3={:016x}", t.pc, cpu.steps, t.gpr[3]);
    for ev in trace.iter() {
        println!("{:08x}: {}", ev.pc, ev.disasm);
    }
    Ok(())
}

fn main() -> Result<()> {
    n64_common::init_logging();
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_help();
        return Ok(());
    }
    match args[1].as_str() {
        "info" if args.len() == 3 => cmd_info(&args[2]),
        "step" if args.len() == 4 => {
            let n: usize = args[3]
                .parse()
                .map_err(|_| EmuError::Other("bad step count".into()))?;
            cmd_step(&args[2], n)
        }
        "scan" => {
            let cfg = EmuConfig::default();
            let games = scan_library(&cfg)?;
            if games.is_empty() {
                println!("no entries (content dir missing or empty)");
            }
            for g in &games {
                println!("{} [{}]", g.title, g.status);
            }
            Ok(())
        }
        _ => {
            print_help();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_missing_dir_is_empty() {
        let cfg = EmuConfig {
            content_dir: PathBuf::from("definitely-not-here-n64"),
            ..EmuConfig::default()
        };
        assert!(scan_library(&cfg).unwrap().is_empty());
    }
}
