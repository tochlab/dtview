use std::error::Error;
use std::fmt;

use super::{
    Dtb, FDT_BEGIN_NODE, FDT_END, FDT_END_NODE, FDT_MAGIC, FDT_NOP, FDT_PROP, Header, Node,
    Property, Reservation,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    BadMagic(u32),
    HeaderTooShort(usize),
    BadStringOffset(usize),
    Truncated(&'static str),
    BadToken(u32),
    UnbalancedNodes,
    UnterminatedString,
    UnsupportedVersion {
        version: u32,
        last_comp_version: u32,
    },
    InvalidHeader(&'static str),
    InvalidSectionLayout,
    InvalidPadding,
    PropertiesAfterChildren,
    InvalidEnd,
    InvalidRoot,
    MissingEnd,
    UnterminatedReservations,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::BadMagic(m) => write!(f, "invalid DTB magic: 0x{m:08x}"),
            ParseError::HeaderTooShort(len) => write!(f, "DTB header too short: {len} bytes"),
            ParseError::BadStringOffset(off) => {
                write!(f, "property name offset {off} outside strings block")
            }
            ParseError::Truncated(what) => write!(f, "truncated DTB while reading {what}"),
            ParseError::BadToken(t) => write!(f, "unknown structure token 0x{t:08x}"),
            ParseError::UnbalancedNodes => {
                write!(f, "unbalanced node begin/end in structure block")
            }
            ParseError::UnterminatedString => {
                write!(f, "unterminated string in structure block")
            }
            ParseError::UnsupportedVersion {
                version,
                last_comp_version,
            } => write!(
                f,
                "unsupported DTB version {version} (backwards-compatible with {last_comp_version})"
            ),
            ParseError::InvalidHeader(field) => write!(f, "invalid DTB header field: {field}"),
            ParseError::InvalidSectionLayout => write!(f, "invalid DTB section layout"),
            ParseError::InvalidPadding => write!(f, "non-zero DTB structure padding"),
            ParseError::PropertiesAfterChildren => {
                write!(f, "node properties appear after child nodes")
            }
            ParseError::InvalidEnd => write!(f, "invalid FDT_END position"),
            ParseError::InvalidRoot => {
                write!(f, "root node has a non-empty name")
            }
            ParseError::MissingEnd => write!(f, "structure block is missing FDT_END"),
            ParseError::UnterminatedReservations => {
                write!(f, "unterminated memory reservation block")
            }
        }
    }
}

impl Error for ParseError {}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    fn read_u32(&mut self) -> Result<u32, ParseError> {
        if self.remaining() < 4 {
            return Err(ParseError::Truncated("u32"));
        }
        let b = &self.buf[self.pos..self.pos + 4];
        self.pos += 4;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_u64(&mut self) -> Result<u64, ParseError> {
        if self.remaining() < 8 {
            return Err(ParseError::Truncated("u64"));
        }
        let b = &self.buf[self.pos..self.pos + 8];
        self.pos += 8;
        Ok(u64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn read(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        if self.remaining() < n {
            return Err(ParseError::Truncated("bytes"));
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn read_node_name(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        let nul = self.buf[start..]
            .iter()
            .position(|&byte| byte == 0)
            .ok_or(ParseError::UnterminatedString)?;
        let name = String::from_utf8_lossy(&self.buf[start..start + nul]).into_owned();
        self.pos = align4(start + nul + 1);
        self.check_padding(start + nul + 1)?;
        Ok(name)
    }

    fn check_padding(&self, start: usize) -> Result<(), ParseError> {
        let padding = self
            .buf
            .get(start..self.pos)
            .ok_or(ParseError::Truncated("alignment padding"))?;
        if padding.iter().any(|&byte| byte != 0) {
            return Err(ParseError::InvalidPadding);
        }
        Ok(())
    }

    fn skip_nops(&mut self) -> Result<(), ParseError> {
        while self.remaining() >= 4 {
            let Some(token_bytes) = self.buf.get(self.pos..self.pos + 4) else {
                return Err(ParseError::Truncated("token"));
            };
            let token = u32::from_be_bytes(
                token_bytes
                    .try_into()
                    .map_err(|_| ParseError::Truncated("token"))?,
            );
            if token != FDT_NOP {
                break;
            }
            self.read_u32()?;
        }
        Ok(())
    }
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

pub fn parse(buf: &[u8]) -> Result<Dtb, ParseError> {
    if buf.len() < 36 {
        return Err(ParseError::HeaderTooShort(buf.len()));
    }
    let mut r = Reader::new(buf);
    let magic = r.read_u32()?;
    if magic != FDT_MAGIC {
        return Err(ParseError::BadMagic(magic));
    }
    let total_size = r.read_u32()?;
    let off_dt_struct = r.read_u32()?;
    let off_dt_strings = r.read_u32()?;
    let off_mem_rsvmap = r.read_u32()?;
    let version = r.read_u32()?;
    let last_comp_version = r.read_u32()?;
    let boot_cpuid_phys = r.read_u32()?;
    let size_dt_strings = r.read_u32()?;
    let size_dt_struct = if version >= 17 {
        Some(r.read_u32()?)
    } else {
        None
    };

    if version < 16 || last_comp_version > version || (version >= 17 && last_comp_version > 17) {
        return Err(ParseError::UnsupportedVersion {
            version,
            last_comp_version,
        });
    }
    let header_size: usize = if version >= 17 { 40 } else { 36 };
    if total_size as usize > buf.len() || (total_size as usize) < header_size {
        return Err(ParseError::InvalidHeader("totalsize"));
    }
    let buf = &buf[..total_size as usize];
    if !(off_dt_struct as usize).is_multiple_of(4) || !(off_mem_rsvmap as usize).is_multiple_of(8) {
        return Err(ParseError::InvalidHeader("block alignment"));
    }

    let struct_start = off_dt_struct as usize;
    let strings_start = off_dt_strings as usize;
    if struct_start < header_size
        || strings_start < struct_start
        || strings_start > buf.len()
        || (off_mem_rsvmap as usize) < header_size
        || off_mem_rsvmap as usize >= struct_start
    {
        return Err(ParseError::InvalidSectionLayout);
    }
    let struct_size = if let Some(size) = size_dt_struct {
        size as usize
    } else {
        infer_struct_size(&buf[struct_start..strings_start])?
    };
    let struct_end = struct_start
        .checked_add(struct_size)
        .ok_or(ParseError::InvalidSectionLayout)?;
    let strings_end = strings_start
        .checked_add(size_dt_strings as usize)
        .ok_or(ParseError::InvalidSectionLayout)?;
    if struct_end > strings_start || strings_end > buf.len() {
        return Err(ParseError::InvalidSectionLayout);
    }
    let struct_buf = &buf[struct_start..struct_end];
    let strings_buf = &buf[strings_start..strings_end];
    let reservations = parse_reservations(&buf[off_mem_rsvmap as usize..struct_start])?;

    let name_at = |off: usize| -> Result<String, ParseError> {
        if off >= strings_buf.len() {
            return Err(ParseError::BadStringOffset(off));
        }
        let s = &strings_buf[off..];
        let nul = s
            .iter()
            .position(|&b| b == 0)
            .ok_or(ParseError::UnterminatedString)?;
        Ok(String::from_utf8_lossy(&s[..nul]).into_owned())
    };

    let root = parse_structure(struct_buf, &name_at)?;
    Ok(Dtb {
        header: Header {
            total_size,
            off_dt_struct,
            off_dt_strings,
            off_mem_rsvmap,
            version,
            last_comp_version,
            boot_cpuid_phys,
            size_dt_strings,
            size_dt_struct,
        },
        reservations,
        root,
    })
}

fn parse_reservations(buf: &[u8]) -> Result<Vec<Reservation>, ParseError> {
    let mut r = Reader::new(buf);
    let mut entries = Vec::new();
    loop {
        if r.remaining() < 16 {
            return Err(ParseError::UnterminatedReservations);
        }
        let address = r.read_u64()?;
        let size = r.read_u64()?;
        if address == 0 && size == 0 {
            return Ok(entries);
        }
        entries.push(Reservation { address, size });
    }
}

fn infer_struct_size(buf: &[u8]) -> Result<usize, ParseError> {
    let mut reader = Reader::new(buf);
    let mut depth = 0usize;
    loop {
        reader.skip_nops()?;
        match reader.read_u32()? {
            FDT_BEGIN_NODE => {
                reader.read_node_name()?;
                depth += 1;
            }
            FDT_END_NODE => {
                if depth == 0 {
                    return Err(ParseError::UnbalancedNodes);
                }
                depth -= 1;
            }
            FDT_PROP => {
                let len = reader.read_u32()? as usize;
                reader.read_u32()?;
                reader.read(len)?;
                reader.pos = align4(reader.pos);
            }
            FDT_END => {
                if depth != 0 {
                    return Err(ParseError::UnbalancedNodes);
                }
                return Ok(reader.pos);
            }
            token => return Err(ParseError::BadToken(token)),
        }
    }
}

fn parse_structure(
    buf: &[u8],
    name_at: &dyn Fn(usize) -> Result<String, ParseError>,
) -> Result<Node, ParseError> {
    let mut r = Reader::new(buf);
    r.skip_nops()?;
    let token = r.read_u32()?;
    if token != FDT_BEGIN_NODE {
        return Err(ParseError::BadToken(token));
    }
    let mut root = parse_node_body(&mut r, name_at)?;
    if !root.name.is_empty() {
        return Err(ParseError::InvalidRoot);
    }
    root.name = "/".into();
    if r.remaining() < 4 {
        return Err(ParseError::MissingEnd);
    }
    if r.read_u32()? != FDT_END {
        return Err(ParseError::MissingEnd);
    }
    if r.remaining() != 0 {
        return Err(ParseError::InvalidEnd);
    }
    Ok(root)
}

fn parse_node_body(
    r: &mut Reader,
    name_at: &dyn Fn(usize) -> Result<String, ParseError>,
) -> Result<Node, ParseError> {
    let name = r.read_node_name()?;

    let mut node = Node {
        name,
        properties: Vec::new(),
        children: Vec::new(),
    };

    let mut has_children = false;
    loop {
        r.skip_nops()?;
        let token = r.read_u32()?;
        match token {
            FDT_END_NODE => return Ok(node),
            FDT_PROP => {
                if has_children {
                    return Err(ParseError::PropertiesAfterChildren);
                }
                let len = r.read_u32()? as usize;
                let nameoff = r.read_u32()? as usize;
                let value_start = r.pos;
                let value = r.read(len)?.to_vec();
                r.pos = align4(r.pos);
                r.check_padding(value_start + len)?;
                let name = name_at(nameoff)?;
                node.properties.push(Property { name, value });
            }
            FDT_BEGIN_NODE => {
                has_children = true;
                let child = parse_node_body(r, name_at)?;
                node.children.push(child);
            }
            FDT_NOP => continue,
            FDT_END => return Err(ParseError::UnbalancedNodes),
            other => return Err(ParseError::BadToken(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32ax(v: u32) -> [u8; 4] {
        v.to_be_bytes()
    }

    fn build_dtb() -> Vec<u8> {
        let mut s = Vec::new();
        let mut add_str = |name: &str| {
            let off = s.len();
            s.extend_from_slice(name.as_bytes());
            s.push(0);
            off
        };
        let addr_cells = add_str("#address-cells");
        let reg = add_str("reg");
        let status = add_str("status");
        let strings = s;

        let mut struct_buf: Vec<u8> = Vec::new();
        let push_token = |buf: &mut Vec<u8>, val: u32| {
            buf.extend_from_slice(&u32ax(val));
        };
        push_token(&mut struct_buf, FDT_BEGIN_NODE);
        struct_buf.push(0);
        struct_buf.resize(align4(struct_buf.len()), 0);

        push_token(&mut struct_buf, FDT_PROP);
        push_token(&mut struct_buf, 8);
        push_token(&mut struct_buf, addr_cells as u32);
        push_token(&mut struct_buf, 1);
        push_token(&mut struct_buf, 1);

        push_token(&mut struct_buf, FDT_BEGIN_NODE);
        struct_buf.extend_from_slice(b"uart@1000\0");
        struct_buf.resize(align4(struct_buf.len()), 0);

        push_token(&mut struct_buf, FDT_PROP);
        push_token(&mut struct_buf, 8);
        push_token(&mut struct_buf, reg as u32);
        push_token(&mut struct_buf, 0x1000);
        push_token(&mut struct_buf, 0x100);

        push_token(&mut struct_buf, FDT_PROP);
        push_token(&mut struct_buf, 4);
        push_token(&mut struct_buf, status as u32);
        push_token(&mut struct_buf, 0x6f6b6179);

        push_token(&mut struct_buf, FDT_END_NODE);
        push_token(&mut struct_buf, FDT_END_NODE);
        push_token(&mut struct_buf, FDT_END);

        let off_mem_rsvmap = 40;
        let off_dt_struct = 72;
        let off_dt_strings = off_dt_struct + struct_buf.len();

        let mut out = Vec::new();
        let totalsize = align4(off_dt_strings) + strings.len();
        out.extend_from_slice(&u32ax(FDT_MAGIC));
        out.extend_from_slice(&u32ax(totalsize as u32));
        out.extend_from_slice(&u32ax(off_dt_struct as u32));
        out.extend_from_slice(&u32ax(off_dt_strings as u32));
        out.extend_from_slice(&u32ax(off_mem_rsvmap as u32));
        out.extend_from_slice(&u32ax(17));
        out.extend_from_slice(&u32ax(16));
        out.extend_from_slice(&u32ax(0));
        out.extend_from_slice(&u32ax(strings.len() as u32));
        out.extend_from_slice(&u32ax(struct_buf.len() as u32));
        out.extend_from_slice(&0u64.to_be_bytes());
        out.extend_from_slice(&0u64.to_be_bytes());
        out.extend_from_slice(&0u64.to_be_bytes());
        out.extend_from_slice(&0u64.to_be_bytes());
        out.extend_from_slice(&struct_buf);
        out.extend_from_slice(&strings);
        out
    }

    #[test]
    fn parses_simple_tree() {
        let dtb = build_dtb();
        let root = parse(&dtb).unwrap().root;
        assert_eq!(root.name, "/");
        assert_eq!(root.properties.len(), 1);
        assert_eq!(root.properties[0].name, "#address-cells");
        assert_eq!(root.children.len(), 1);
        let uart = &root.children[0];
        assert_eq!(uart.name, "uart@1000");
        assert_eq!(uart.properties.len(), 2);
        assert_eq!(uart.properties[0].name, "reg");
        assert_eq!(
            uart.properties[0].value,
            [0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x01, 0x00]
        );
        assert_eq!(uart.properties[1].name, "status");
        assert_eq!(uart.properties[1].value, b"okay");
    }

    #[test]
    fn rejects_bad_magic() {
        let mut dtb = build_dtb();
        dtb[0..4].copy_from_slice(&0xdeadbeefu32.to_be_bytes());
        assert!(matches!(parse(&dtb), Err(ParseError::BadMagic(0xdeadbeef))));
    }

    #[test]
    fn rejects_short_header() {
        assert!(matches!(
            parse(&[0u8; 10]),
            Err(ParseError::HeaderTooShort(_))
        ));
    }

    #[test]
    fn parses_reservation_entries_and_ignores_trailing_bytes() {
        let mut dtb = build_dtb();
        dtb[40..48].copy_from_slice(&0x0000000180000000u64.to_be_bytes());
        dtb[48..56].copy_from_slice(&0x0000000000002000u64.to_be_bytes());
        dtb.extend_from_slice(b"ignored");

        let parsed = parse(&dtb).unwrap();
        assert_eq!(parsed.reservations.len(), 1);
        assert_eq!(parsed.reservations[0].address, 0x180000000);
        assert_eq!(parsed.reservations[0].size, 0x2000);
    }

    #[test]
    fn accepts_version_16_without_size_dt_struct() {
        let mut dtb = build_dtb();
        dtb[20..24].copy_from_slice(&16u32.to_be_bytes());
        dtb[24..28].copy_from_slice(&16u32.to_be_bytes());
        let strings_offset = u32::from_be_bytes(dtb[12..16].try_into().unwrap()) as usize;
        dtb.splice(56..72, std::iter::empty());
        let struct_offset = u32::from_be_bytes(dtb[8..12].try_into().unwrap());
        dtb[8..12].copy_from_slice(&(struct_offset - 16).to_be_bytes());
        dtb[12..16].copy_from_slice(&((strings_offset - 16) as u32).to_be_bytes());
        let total_size = dtb.len() as u32;
        dtb[4..8].copy_from_slice(&total_size.to_be_bytes());
        let parsed = parse(&dtb).unwrap();
        assert_eq!(parsed.header.version, 16);
        assert_eq!(parsed.header.size_dt_struct, None);
    }

    #[test]
    fn rejects_unterminated_reservation_list() {
        let mut dtb = build_dtb();
        dtb[40..72].fill(1);
        assert_eq!(parse(&dtb), Err(ParseError::UnterminatedReservations));
    }

    #[test]
    fn rejects_incomplete_node_name_padding_without_panicking() {
        let mut dtb = build_dtb();
        let struct_start = u32::from_be_bytes(dtb[8..12].try_into().unwrap()) as usize;
        let struct_end =
            struct_start + u32::from_be_bytes(dtb[36..40].try_into().unwrap()) as usize;
        dtb.splice(struct_end - 4..struct_end, [0, 0, 0, 1]);
        let struct_size = u32::from_be_bytes(dtb[36..40].try_into().unwrap()) - 3;
        dtb[36..40].copy_from_slice(&struct_size.to_be_bytes());
        assert!(parse(&dtb).is_err());
    }

    #[test]
    fn rejects_nonzero_structure_padding() {
        let mut dtb = build_dtb();
        let structure_start = u32::from_be_bytes(dtb[8..12].try_into().unwrap()) as usize;
        dtb[structure_start + 5] = 1;
        assert_eq!(parse(&dtb), Err(ParseError::InvalidPadding));
    }

    #[test]
    fn rejects_tokens_after_fdt_end() {
        let mut dtb = build_dtb();
        let struct_start = u32::from_be_bytes(dtb[8..12].try_into().unwrap()) as usize;
        let struct_size = u32::from_be_bytes(dtb[36..40].try_into().unwrap()) as usize;
        let end = struct_start + struct_size;
        dtb.splice(end..end, [0, 0, 0, FDT_NOP as u8]);
        let struct_size = (struct_size + 4) as u32;
        dtb[36..40].copy_from_slice(&struct_size.to_be_bytes());
        let strings_offset = u32::from_be_bytes(dtb[12..16].try_into().unwrap()) + 4;
        dtb[12..16].copy_from_slice(&strings_offset.to_be_bytes());
        let total_size = dtb.len() as u32;
        dtb[4..8].copy_from_slice(&total_size.to_be_bytes());
        assert_eq!(parse(&dtb), Err(ParseError::InvalidEnd));
    }

    #[test]
    fn parses_repository_dtb() {
        let parsed = parse(include_bytes!("../../real.dtb")).unwrap();
        assert_eq!(parsed.root.name, "/");
        assert!(!parsed.root.children.is_empty());
    }
}
