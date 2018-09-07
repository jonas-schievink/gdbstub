//! Target platform definitions.

use Comm;

use byteorder::ByteOrder;

macro_rules! def_regs {
    (
        $( #[$attr:meta] )*
        pub struct $name:ident {
            $( $reg:ident : $t:ty, )+
        }
    ) => {
        $( #[$attr] )*
        #[derive(Debug)]
        pub struct $name {
            $( pub $reg: $t, )+
        }

        impl ::targets::EncodeRegister for $name {
            fn encode<C: ::Comm, B: ::byteorder::ByteOrder>(&self, comm: &mut C) -> Result<(), C::Error> {
                $(
                    self.$reg.encode::<C, B>(comm)?;
                )+
                Ok(())
            }
        }
    };
}

/// Trait for target machine descriptions.
pub trait TargetDesc {
    /// A structure containing the target's register values, as expected by GDB.
    ///
    /// These can be extracted from `https://github.com/gergap/binutils-gdb/tree/2b8118237ae25785e3afddafd9c554b1ad03d424/gdb/features`.
    type Registers: EncodeRegister;

    /// The target endianness.
    type Endianness: ByteOrder;
}

/// Trait for register values and structs.
///
/// This is used to encode the target-specific register struct so that the
/// connected debugger can understand it.
pub trait EncodeRegister {
    /// Encode the register value(s) of `self` as hexadecimal strings and send
    /// them via `comm`.
    ///
    /// `B` specifies the endianness to use and is set to the target's native
    /// endianness by the library.
    fn encode<C: Comm, B: ByteOrder>(&self, comm: &mut C) -> Result<(), C::Error>;
}

impl EncodeRegister for u32 {
    fn encode<C: Comm, B: ByteOrder>(&self, comm: &mut C) -> Result<(), C::Error> {
        let mut buf = [0; 4];
        B::write_u32(&mut buf, *self);
        comm.write_all_hex(&buf)
    }
}

impl EncodeRegister for u64 {
    fn encode<C: Comm, B: ByteOrder>(&self, comm: &mut C) -> Result<(), C::Error> {
        let mut buf = [0; 8];
        B::write_u64(&mut buf, *self);
        comm.write_all_hex(&buf)
    }
}

impl EncodeRegister for u128 {
    fn encode<C: Comm, B: ByteOrder>(&self, comm: &mut C) -> Result<(), C::Error> {
        let mut buf = [0; 16];
        B::write_u128(&mut buf, *self);
        comm.write_all_hex(&buf)
    }
}

impl EncodeRegister for [u8; 10] {
    fn encode<C: Comm, B: ByteOrder>(&self, comm: &mut C) -> Result<(), C::Error> {
        // FIXME swap endianness?
        comm.write_all_hex(self)
    }
}

/// Does nothing.
impl EncodeRegister for () {
    fn encode<C: Comm, B: ByteOrder>(&self, _comm: &mut C) -> Result<(), C::Error> {
        Ok(())
    }
}

/// The Intel x86 family of processors.
pub mod x86 {
    /// 32-bit x86.
    pub struct I386;

    impl super::TargetDesc for I386 {
        type Registers = X86Registers;
        type Endianness = ::byteorder::LittleEndian;
    }

    def_regs! {
        /// Register contents of a 32-bit x86 processor.
        ///
        /// This assumes SSE support. If your target doesn't support SSE, leave
        /// the registers set to 0.
        // FIXME: There's probably a difference between 0 and "not transmitted"
        pub struct X86Registers {
            eax: u32,
            ebx: u32,
            ecx: u32,
            edx: u32,
            esp: u32,
            ebp: u32,
            esi: u32,
            edi: u32,

            eip: u32,
            eflags: u32,
            cs: u32,
            ss: u32,
            ds: u32,
            es: u32,
            fs: u32,
            gs: u32,

            st0: [u8; 10],
            st1: [u8; 10],
            st2: [u8; 10],
            st3: [u8; 10],
            st4: [u8; 10],
            st5: [u8; 10],
            st6: [u8; 10],
            st7: [u8; 10],
            fctrl: u32,
            fstat: u32,
            ftag: u32,
            fiseg: u32,
            fioff: u32,
            foseg: u32,
            fooff: u32,
            fop: u32,

            xmm0: u128,
            xmm1: u128,
            xmm2: u128,
            xmm3: u128,
            xmm4: u128,
            xmm5: u128,
            xmm6: u128,
            xmm7: u128,
            mxcsr: u32,
        }
    }
    // FIXME how to handle extensions like MMX/SSE/...?
}
