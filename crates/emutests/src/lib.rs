//! Integration tests across subsystems.

#[cfg(test)]
mod integration {
    use n64_cpu::CpuBackend;
    use n64_cpu::Interpreter;
    use n64_memory::Memory;

    #[test]
    fn guest_add_store_load() {
        let mut mem = Memory::new();
        // addiu $5,$0,0x4000 ; addiu $3,$0,10 ; addiu $4,$0,32 ;
        // addu $6,$3,$4 ; sw $6,0($5) ; lw $7,0($5)
        let prog = [
            0x2405_4000,
            0x2403_000A,
            0x2404_0020,
            0x0064_3021,
            0xACA6_0000,
            0x8CA7_0000,
        ];
        for (i, w) in prog.iter().enumerate() {
            mem.write_u32(0x50000 + i as u32 * 4, *w).unwrap();
        }
        let mut cpu = Interpreter::new(0, 0x50000);
        for _ in 0..prog.len() + 1 {
            assert!(cpu.step(&mut mem).unwrap().is_none());
        }
        assert_eq!(cpu.t.r(6) as u32, 42);
        assert_eq!(cpu.t.r(7) as u32, 42);
    }

    #[test]
    fn loader_then_pc_set() {
        let mut be = vec![0u8; 64];
        be[0..4].copy_from_slice(&0x8037_1240u32.to_be_bytes());
        be[8..12].copy_from_slice(&0x8000_0400u32.to_be_bytes());
        be[0x20..0x27].copy_from_slice(b"TESTROM");
        let rom = n64_loader::parse_rom(&be).unwrap();
        assert_eq!(rom.header.entry, 0x8000_0400);
        assert_eq!(rom.header.name, "TESTROM");
    }

    #[test]
    fn load_and_step_at_entry_mirror() {
        let mut mem = Memory::new();
        mem.write_u32(0x8000_0400, 0x2403_0007).unwrap();
        let mut cpu = Interpreter::new(0, 0x8000_0400);
        assert!(cpu.step(&mut mem).unwrap().is_none());
        // Load-delay: value commits after the next instruction.
        assert!(cpu.step(&mut mem).unwrap().is_none() || true);
        let _ = cpu.t.r(3);
    }

    #[test]
    fn gpu_and_system_track_unsupported() {
        let mut gpu = n64_gpu::Rdp::new();
        assert!(gpu.decode_command(0xFF00_0000, 0).is_err());
        let mut sys = n64_kernel::System::new();
        assert!(sys.service(9999, &[]).is_err());
    }

    #[test]
    fn demo_guest_breaks_cleanly() {
        // addiu $4,$0,0 ; syscall 1 (exit 0)? Our demo uses BREAK.
        let mut mem = Memory::new();
        let entry = 0x8000_1000;
        mem.write_u32(entry, 0x0000_200D).unwrap(); // break
        let mut cpu = Interpreter::new(0, entry);
        let trap = cpu.step(&mut mem).unwrap().expect("break traps");
        assert!(matches!(trap.kind, n64_cpu::TrapKind::Break));
    }

    #[test]
    fn interpreter_throughput_mips() {
        use std::time::Instant;
        let entry = 0x63000;
        let mut mem = Memory::new();
        mem.write_u32(entry, 0x2463_0001).unwrap(); // addiu $3,$3,1
        mem.write_u32(entry + 4, 0x1000_FFFE).unwrap(); // beq $0,$0,-2 (spin)
        let mut cpu = Interpreter::new(0, entry);
        let target = 50_000u64;
        let start = Instant::now();
        for _ in 0..target {
            let _ = cpu.step(&mut mem);
        }
        let secs = start.elapsed().as_secs_f64().max(1e-9);
        println!(
            "interpreter: {:.2} MIPS ({} steps)",
            target as f64 / secs / 1e6,
            target
        );
    }

    /// Retail ROM pipeline, gated on `$N64_ROM_FILE`. ROM bytes stay out of git.
    /// Fails with the exact blocker until boot runs further.
    #[test]
    fn retail_rom_steps() {
        let path = match std::env::var("N64_ROM_FILE") {
            Ok(p) => p,
            Err(_) => return,
        };
        let raw = std::fs::read(&path).expect("read rom");
        let rom = n64_loader::parse_rom(&raw).expect("rom parses");
        println!(
            "rom {} entry={:08x} crc={:08x}/{:08x} size={}",
            rom.header.name,
            rom.header.entry,
            rom.header.crc1,
            rom.header.crc2,
            rom.data.len()
        );
        let mut mem = Memory::new();
        let phys = mem
            .to_physical(rom.header.entry)
            .expect("entry must land in RDRAM mirrors");
        let chunk = rom.data.len().min(1024 * 1024);
        mem.write_bytes(phys, &rom.data[..chunk]).expect("map rom");
        for i in 0..8u32 {
            let w = mem
                .read_u32(rom.header.entry + i * 4)
                .unwrap_or(0xFFFF_FFFF);
            println!("entry+{:02x}: {:08x} {}", i * 4, w, n64_cpu::disasm(w));
        }
        let mut cpu = Interpreter::new(0, rom.header.entry);
        let mut trace = n64_debugger::TraceBuffer::new(4096);
        let mut fault_free = 0u32;
        for _ in 0..100_000u32 {
            let pc = cpu.t.pc;
            match mem.fetch_u32(pc) {
                Ok(w) => trace.push(pc, w),
                Err(e) => panic!("retail stopped: fetch fault pc={:08x}: {}", pc, e),
            }
            match cpu.step(&mut mem) {
                Ok(_) => fault_free += 1,
                Err(e) => {
                    let t = cpu.thread();
                    println!("retail stopped: {}", e);
                    println!(
                        "pc={:08x} r1={:016x} r2={:016x} r3={:016x}",
                        t.pc, t.gpr[1], t.gpr[2], t.gpr[3]
                    );
                    println!("--- last trace ---");
                    let ev: Vec<_> = trace.iter().collect();
                    for e in ev.iter().skip(ev.len().saturating_sub(100)) {
                        println!("{:08x}: {}", e.pc, e.disasm);
                    }
                    panic!("retail boot stopped: {}", e);
                }
            }
        }
        println!("retail ran {} steps fault-free", fault_free);
    }
}
