//! This example shows basic usage of a TCP-based `gdbstub`.

extern crate gdbstub;
extern crate env_logger;

use std::net::TcpListener;
use gdbstub::{GdbStub, StubCalls};
use gdbstub::targets::x86;

const MEMORY: &'static [u8] = &[
    0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, // 0x7
    0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, // 0xf
    0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, // 0x17
    0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, // 0x1f
    0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, // 0x27
    0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, // 0x2f
    0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, // 0x37
    0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, // 0x3f
];

/// This struct implements the debugger access to our target system.
struct DummyTarget<'a> {
    mem: &'a mut [u8],
}

impl<'a> StubCalls for DummyTarget<'a> {
    type Target = x86::I386;

    fn read_registers(&mut self) -> x86::X86Registers {
        x86::X86Registers {
            eax: 0,
            ebx: 0,
            ecx: 0,
            edx: 0,
            esp: 0,
            ebp: 0,
            esi: 0,
            edi: 0,
            eip: 0x10,
            eflags: 0,
            cs: 0,
            ss: 0,
            ds: 0,
            es: 0,
            fs: 0,
            gs: 0,
            st0: [0; 10],
            st1: [0; 10],
            st2: [0; 10],
            st3: [0; 10],
            st4: [0; 10],
            st5: [0; 10],
            st6: [0; 10],
            st7: [0; 10],
            fctrl: 0,
            fstat: 0,
            ftag: 0,
            fiseg: 0,
            fioff: 0,
            foseg: 0,
            fooff: 0,
            fop: !0,
        }
    }

    fn read_mem(&mut self, addr: u64) -> Result<u8, ()> {
        if addr < self.mem.len() as u64 {
            Ok(self.mem[addr as usize])
        } else {
            Err(())
        }
    }

    fn write_mem(&mut self, addr: u64, byte: u8) -> Result<(), ()> {
        if addr < self.mem.len() as u64 {
            self.mem[addr as usize] = byte;
            Ok(())
        } else {
            Err(())
        }
    }
}

fn main() {
    env_logger::init();

    // Wait for GDB connection:
    let (stream, addr) = TcpListener::bind("127.0.0.1:9001").unwrap().accept().unwrap();
    println!("Incoming Connection from {}", addr);

    let mut mem = Vec::from(MEMORY);
    let stub = GdbStub::new(stream, DummyTarget {
        mem: &mut mem,
    });

    match stub.poll() {
        Ok(()) => {}
        Err(e) => eprintln!("Lost debugger connection: {:?}", e),
    }
}
