use std::collections::HashMap;

use super::{Node, Property};

#[derive(Debug)]
pub struct PhandleMap {
    providers: HashMap<u32, Provider>,
}

#[derive(Debug)]
struct Provider {
    path: String,
    cells: HashMap<&'static str, usize>,
}

impl PhandleMap {
    pub fn from_tree(root: &Node) -> Self {
        let mut providers = HashMap::new();
        collect_providers(root, String::new(), &mut providers);
        Self { providers }
    }

    pub fn resolve_property(&self, property: &Property) -> Option<Vec<String>> {
        let value = cells(&property.value)?;
        let schema = tuple_schema(&property.name);
        if schema.is_none()
            && !is_single_reference(&property.name)
            && !is_reference_list(&property.name)
        {
            return None;
        }
        let mut resolved = Vec::new();
        let mut index = 0;

        while index < value.len() {
            let handle = value[index];
            let provider = self.providers.get(&handle)?;
            resolved.push(provider.path.clone());
            index += 1;

            if let Some(cell_name) = schema {
                index += *provider.cells.get(cell_name)?;
                if index > value.len() {
                    return None;
                }
            } else if is_single_reference(&property.name) && index != value.len() {
                return None;
            }
        }

        if resolved.is_empty() {
            None
        } else {
            Some(resolved)
        }
    }
}

fn collect_providers(node: &Node, path: String, providers: &mut HashMap<u32, Provider>) {
    let mut cell_counts = HashMap::new();
    for (property, cell_name) in [
        ("#clock-cells", "#clock-cells"),
        ("#reset-cells", "#reset-cells"),
        ("#dma-cells", "#dma-cells"),
        ("#phy-cells", "#phy-cells"),
        ("#power-domain-cells", "#power-domain-cells"),
        ("#iommu-cells", "#iommu-cells"),
        ("#interrupt-cells", "#interrupt-cells"),
        ("#gpio-cells", "#gpio-cells"),
        ("#pwm-cells", "#pwm-cells"),
        ("#mbox-cells", "#mbox-cells"),
        ("#io-channel-cells", "#io-channel-cells"),
        ("#interconnect-cells", "#interconnect-cells"),
        ("#sound-dai-cells", "#sound-dai-cells"),
        ("#cooling-cells", "#cooling-cells"),
        ("#nvmem-cell-cells", "#nvmem-cell-cells"),
        ("#msi-cells", "#msi-cells"),
    ] {
        if let Some(count) = node
            .properties
            .iter()
            .find(|item| item.name == property)
            .and_then(|item| cells(&item.value))
            .and_then(|value| value.first().copied())
        {
            cell_counts.insert(cell_name, count as usize);
        }
    }

    for property in &node.properties {
        if matches!(property.name.as_str(), "phandle" | "linux,phandle")
            && let Some(handle) = cells(&property.value).and_then(|value| value.first().copied())
        {
            providers.insert(
                handle,
                Provider {
                    path: if path.is_empty() {
                        "/".into()
                    } else {
                        path.clone()
                    },
                    cells: cell_counts.clone(),
                },
            );
        }
    }

    for child in &node.children {
        let child_path = if path.is_empty() {
            format!("/{}", child.name)
        } else {
            format!("{path}/{}", child.name)
        };
        collect_providers(child, child_path, providers);
    }
}

fn cells(value: &[u8]) -> Option<Vec<u32>> {
    if value.is_empty() || !value.len().is_multiple_of(4) {
        return None;
    }
    Some(
        value
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| u32::from_be_bytes(*chunk))
            .collect(),
    )
}

fn tuple_schema(name: &str) -> Option<&'static str> {
    match name {
        "clocks" => Some("#clock-cells"),
        "resets" => Some("#reset-cells"),
        "dmas" => Some("#dma-cells"),
        "phys" => Some("#phy-cells"),
        "power-domains" => Some("#power-domain-cells"),
        "iommus" => Some("#iommu-cells"),
        "interrupts-extended" => Some("#interrupt-cells"),
        "gpios" | "gpio" => Some("#gpio-cells"),
        "pwms" => Some("#pwm-cells"),
        "mboxes" => Some("#mbox-cells"),
        "io-channels" => Some("#io-channel-cells"),
        "interconnects" => Some("#interconnect-cells"),
        "sound-dai" => Some("#sound-dai-cells"),
        "cooling-device" => Some("#cooling-cells"),
        "nvmem-cells" => Some("#nvmem-cell-cells"),
        "msi-parent" => Some("#msi-cells"),
        _ if name.ends_with("-gpios") || name.ends_with("-gpio") => Some("#gpio-cells"),
        _ => None,
    }
}

fn is_reference_list(name: &str) -> bool {
    matches!(name, "pinctrl-0" | "pinctrl-1" | "pinctrl-2" | "pinctrl-3")
}

fn is_single_reference(name: &str) -> bool {
    matches!(
        name,
        "interrupt-parent"
            | "memory-region"
            | "remote-endpoint"
            | "phy-handle"
            | "operating-points-v2"
            | "cpu"
            | "next-level-cache"
    ) || name.ends_with("-supply")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn property(name: &str, values: &[u32]) -> Property {
        Property {
            name: name.into(),
            value: values
                .iter()
                .flat_map(|value| value.to_be_bytes())
                .collect(),
        }
    }

    fn node(name: &str, properties: Vec<Property>, children: Vec<Node>) -> Node {
        Node {
            name: name.into(),
            properties,
            children,
        }
    }

    #[test]
    fn resolves_single_reference() {
        let root = node(
            "/",
            vec![],
            vec![
                node("consumer", vec![property("interrupt-parent", &[7])], vec![]),
                node(
                    "interrupt-controller",
                    vec![property("phandle", &[7])],
                    vec![],
                ),
            ],
        );
        let map = PhandleMap::from_tree(&root);
        assert_eq!(
            map.resolve_property(&root.children[0].properties[0]),
            Some(vec!["/interrupt-controller".into()])
        );
    }

    #[test]
    fn resolves_provider_tuples_and_arguments() {
        let root = node(
            "/",
            vec![],
            vec![
                node("consumer", vec![property("clocks", &[7, 2, 9, 8])], vec![]),
                node(
                    "clock-controller",
                    vec![property("phandle", &[7]), property("#clock-cells", &[3])],
                    vec![],
                ),
            ],
        );
        let map = PhandleMap::from_tree(&root);
        assert_eq!(
            map.resolve_property(&root.children[0].properties[0]),
            Some(vec!["/clock-controller".into()])
        );
    }

    #[test]
    fn leaves_regular_cell_properties_unresolved() {
        let root = node(
            "/",
            vec![],
            vec![node("device", vec![property("reg", &[7, 0x1000])], vec![])],
        );
        let map = PhandleMap::from_tree(&root);
        assert_eq!(map.resolve_property(&root.children[0].properties[0]), None);
    }
}
