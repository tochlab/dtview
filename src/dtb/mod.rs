pub mod parser;
pub mod phandle;
pub mod value;

pub const FDT_BEGIN_NODE: u32 = 0x1;
pub const FDT_END_NODE: u32 = 0x2;
pub const FDT_PROP: u32 = 0x3;
pub const FDT_NOP: u32 = 0x4;
pub const FDT_END: u32 = 0x9;
pub const FDT_MAGIC: u32 = 0xd00dfeed;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dtb {
    pub header: Header,
    pub reservations: Vec<Reservation>,
    pub root: Node,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub total_size: u32,
    pub off_dt_struct: u32,
    pub off_dt_strings: u32,
    pub off_mem_rsvmap: u32,
    pub version: u32,
    pub last_comp_version: u32,
    pub boot_cpuid_phys: u32,
    pub size_dt_strings: u32,
    pub size_dt_struct: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reservation {
    pub address: u64,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub properties: Vec<Property>,
    pub children: Vec<Node>,
}
