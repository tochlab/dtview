use std::fmt::Write;

pub fn is_printable(value: &[u8]) -> bool {
    value.iter().all(|&b| (0x20..=0x7e).contains(&b) || b == 0)
}

pub fn all_cells(value: &[u8]) -> Option<Vec<u32>> {
    if value.is_empty() || !value.len().is_multiple_of(4) {
        return None;
    }
    Some(
        value
            .as_chunks::<4>()
            .0
            .iter()
            .map(|cell| u32::from_be_bytes(*cell))
            .collect(),
    )
}

pub fn format_string_list(value: &[u8]) -> String {
    let parts: Vec<String> = value
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    if parts.len() == 1 {
        parts[0].clone()
    } else {
        parts.join(", ")
    }
}

pub fn format_bytes(value: &[u8]) -> String {
    let mut out = String::new();
    for &b in value.iter().take(16) {
        write!(&mut out, "{b:02x} ").unwrap();
    }
    if value.len() > 16 {
        write!(&mut out, "... ({:?} bytes)", value.len()).unwrap();
    }
    out.trim_end().to_string()
}

pub fn format_hex_u32s(cells: &[u32]) -> String {
    cells
        .iter()
        .map(|c| format!("0x{c:08x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_hex_integer(cells: &[u32]) -> String {
    let Some(first_nonzero) = cells.iter().position(|cell| *cell != 0) else {
        return "0x0".to_string();
    };
    let mut out = format!("0x{:x}", cells[first_nonzero]);
    for cell in &cells[first_nonzero + 1..] {
        write!(&mut out, "{cell:08x}").unwrap();
    }
    out
}

pub fn format_address_property(
    name: &str,
    value: &[u8],
    address_cells: usize,
    parent_address_cells: usize,
    size_cells: usize,
) -> Option<String> {
    let cells = all_cells(value)?;
    let (tuple_width, range) = if name == "reg" {
        (address_cells + size_cells, false)
    } else if name == "ranges" {
        (address_cells + parent_address_cells + size_cells, true)
    } else {
        return None;
    };
    if tuple_width == 0 || !cells.len().is_multiple_of(tuple_width) {
        return None;
    }

    let mut entries = Vec::new();
    for tuple in cells.chunks_exact(tuple_width) {
        let address = format_hex_integer(&tuple[..address_cells]);
        if range {
            let parent_end = address_cells + parent_address_cells;
            let parent_address = format_hex_integer(&tuple[address_cells..parent_end]);
            if size_cells == 0 {
                entries.push(format!("{address} -> {parent_address}"));
            } else {
                let size = format_hex_integer(&tuple[parent_end..]);
                entries.push(format!("{address} -> {parent_address} (size {size})"));
            }
        } else if size_cells == 0 {
            entries.push(address);
        } else {
            let size = format_hex_integer(&tuple[address_cells..]);
            entries.push(format!("{address} (size {size})"));
        }
    }
    Some(entries.join(", "))
}

pub fn format_frequency_property(name: &str, value: &[u8], human_readable: bool) -> Option<String> {
    if !matches!(name, "clock-frequency" | "assigned-clock-rates") {
        return None;
    }
    let cells = all_cells(value)?;
    if cells.is_empty() {
        return None;
    }
    if human_readable {
        Some(
            cells
                .iter()
                .map(|frequency| format!("{frequency} Hz"))
                .collect::<Vec<_>>()
                .join(", "),
        )
    } else {
        Some(format_hex_u32s(&cells))
    }
}

pub struct ValueFormat {
    pub kind: &'static str,
    pub text: String,
}

pub fn interpret(name: &str, value: &[u8]) -> ValueFormat {
    match name {
        "#address-cells" | "#size-cells" | "phandle" | "linux,phandle" => {
            if let Some(cells) = all_cells(value) {
                ValueFormat {
                    kind: "cells",
                    text: format_hex_u32s(&cells),
                }
            } else {
                ValueFormat {
                    kind: "bytes",
                    text: format_bytes(value),
                }
            }
        }
        "compatible" | "status" | "model" => {
            if is_printable(value) {
                ValueFormat {
                    kind: "string",
                    text: format_string_list(value),
                }
            } else {
                ValueFormat {
                    kind: "bytes",
                    text: format_bytes(value),
                }
            }
        }
        "reg" | "ranges" => {
            if let Some(cells) = all_cells(value) {
                ValueFormat {
                    kind: "cells",
                    text: format_hex_u32s(&cells),
                }
            } else {
                ValueFormat {
                    kind: "bytes",
                    text: format_bytes(value),
                }
            }
        }
        _ => {
            if value.is_empty() {
                return ValueFormat {
                    kind: "empty",
                    text: "<empty>".to_string(),
                };
            }
            if is_printable(value) {
                return ValueFormat {
                    kind: "string",
                    text: format_string_list(value),
                };
            }
            if let Some(cells) = all_cells(value) {
                return ValueFormat {
                    kind: "cells",
                    text: format_hex_u32s(&cells),
                };
            }
            ValueFormat {
                kind: "bytes",
                text: format_bytes(value),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_prop() {
        let v = interpret("compatible", b"acme,uart\0acme,uart-v2");
        assert_eq!(v.kind, "string");
        assert_eq!(v.text, "acme,uart, acme,uart-v2");
    }

    #[test]
    fn cells_prop() {
        let v = interpret("phandle", &0x1fu32.to_be_bytes());
        assert_eq!(v.kind, "cells");
        assert_eq!(v.text, "0x0000001f");
    }

    #[test]
    fn reg_prop() {
        let mut val = Vec::new();
        val.extend_from_slice(&0x1000u32.to_be_bytes());
        val.extend_from_slice(&0x100u32.to_be_bytes());
        let v = interpret("reg", &val);
        assert_eq!(v.kind, "cells");
        assert_eq!(v.text, "0x00001000 0x00000100");
    }

    #[test]
    fn empty_prop() {
        let v = interpret("interrupt-controller", &[]);
        assert_eq!(v.kind, "empty");
    }

    #[test]
    fn binary_fallback() {
        let v = interpret("foo", &[0xff, 0xfe, 0x01]);
        assert_eq!(v.kind, "bytes");
        assert_eq!(v.text, "ff fe 01");
    }

    #[test]
    fn non_ascii_becomes_cells() {
        let v = interpret("foo", &[1, 2, 3, 5]);
        assert_eq!(v.kind, "cells");
        assert_eq!(v.text, "0x01020305");
    }

    #[test]
    fn all_cells_odd_length() {
        assert_eq!(all_cells(&[1, 2, 3]), None);
        assert_eq!(all_cells(&[]), None);
    }

    #[test]
    fn formats_reg_address_and_size_as_integers() {
        let value = [0, 0, 0x10, 0, 0, 0, 1, 0];
        assert_eq!(
            format_address_property("reg", &value, 1, 1, 1),
            Some("0x1000 (size 0x100)".to_string())
        );
    }

    #[test]
    fn formats_wide_reg_address_as_one_integer() {
        let value = [0, 0, 0, 0x10, 0, 0, 0x10, 0, 0, 0, 0, 0x20];
        assert_eq!(
            format_address_property("reg", &value, 2, 2, 1),
            Some("0x1000001000 (size 0x20)".to_string())
        );
    }

    #[test]
    fn omits_zero_high_cells_from_wide_address() {
        let value = [0, 0, 0, 0, 0, 0, 0x10, 0, 0, 0, 0, 0x20];
        assert_eq!(
            format_address_property("reg", &value, 2, 2, 1),
            Some("0x1000 (size 0x20)".to_string())
        );
    }

    #[test]
    fn formats_ranges_child_parent_and_size() {
        let value = [0, 0, 0, 0x10, 0, 0, 0, 0x20, 0, 0, 0, 0x10];
        assert_eq!(
            format_address_property("ranges", &value, 1, 1, 1),
            Some("0x10 -> 0x20 (size 0x10)".to_string())
        );
    }

    #[test]
    fn formats_clock_frequencies_as_hertz_in_human_readable_mode() {
        let value = [0x01, 0x6e, 0x36, 0, 0, 0x3d, 0x09, 0];
        assert_eq!(
            format_frequency_property("assigned-clock-rates", &value, true),
            Some("24000000 Hz, 4000000 Hz".to_string())
        );
    }

    #[test]
    fn formats_clock_frequencies_as_hex_in_raw_mode() {
        let value = [0x01, 0x6e, 0x36, 0x80];
        assert_eq!(
            format_frequency_property("clock-frequency", &value, false),
            Some("0x016e3680".to_string())
        );
    }

    #[test]
    fn does_not_interpret_clocks_phandle_arguments_as_frequency() {
        let value = [0, 0, 0, 7, 0, 0, 0, 2];
        assert_eq!(format_frequency_property("clocks", &value, true), None);
    }
}
