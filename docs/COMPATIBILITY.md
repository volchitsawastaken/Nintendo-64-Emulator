# Compatibility Matrix

Statuses: `verified` (tested, evidence), `partial` (some paths work),
`unsupported` (explicit error), `not-started`.

## CPU (VR4300)

| Area | Status | Notes |
|---|---|---|
| Integer ALU (addiu/addi/and/or/xor/slt, addu/add/subu, shifts) | partial | Unit vectors; delay slots + load-delay real |
| Branches (beq/bne/blez/bgtz/j/jal/jr/jalr) | partial | Delay slot executes; link writes r31 |
| Loads/stores (lw/sw/lb/lbu/lh/lhu/sb/sh) | partial | Alignment + bounds checked, BE |
| MULT/MULTU/DIV/DIVU + MFHI/MFLO | partial | DIV-by-zero is loud Unsupported |
| COP0 (Status/Cause/EPC/Count/Compare) | partial | Others explicit Unsupported |
| FPU (COP1) | unsupported | Tracked per-word, required for retail |
| TLB/exceptions | not-started | |
| RSP vector unit | unsupported | `submit_task` loud with ucode/size |

## RDP/GPU

| Area | Status | Notes |
|---|---|---|
| Command decode (sync/fill/scissor/color-image) | partial | Unknown ops loud with raw words |
| Fill + software framebuffer | partial | `validate_fill` reads back pixels |
| TextureRect/sampler | unsupported | Pending, no fake pixels |
| VI timing/interrupts | not-started | |

## Loader/Boot

| Area | Status | Notes |
|---|---|---|
| z64/n64/v64 normalize + header | partial | Magic + entry + CRC + name; tests |
| Boot to entry + stepping | partial | CLI `step` with trace; retail gated on `$N64_ROM_FILE` |
| Saves (EEPROM/SRAM/FlashRAM/Pak) | partial | Sized by hardware truth, file-backed |

## Audio/Network/Frontend

| Area | Status | Notes |
|---|---|---|
| Audio null sink | partial | Host loud Unsupported |
| Network | unsupported | Loopback harness only (no N64 net hardware) |
| Frontend CLI | partial | info/step/scan real; desktop UI at M2+ |
