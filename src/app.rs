use crossterm::event::{KeyCode, KeyEvent};

use crate::dtb::{Dtb, Node, phandle::PhandleMap};

#[derive(Debug)]
pub struct App {
    pub dtb: Dtb,
    pub phandles: PhandleMap,
    pub selected: Vec<usize>,
    pub expanded: Vec<Vec<usize>>,
    pub offset: usize,
    pub human_readable: bool,
}

impl App {
    pub fn new(dtb: Dtb) -> Self {
        let phandles = PhandleMap::from_tree(&dtb.root);
        App {
            dtb,
            phandles,
            selected: vec![],
            expanded: vec![vec![]],
            offset: 0,
            human_readable: false,
        }
    }

    pub fn node_at(&self, path: &[usize]) -> &Node {
        path.iter()
            .fold(&self.dtb.root, |node, &idx| &node.children[idx])
    }

    pub fn selected_node(&self) -> &Node {
        self.node_at(&self.selected)
    }

    pub fn selected_parent_node(&self) -> Option<&Node> {
        if self.selected.is_empty() {
            None
        } else {
            Some(self.node_at(&self.selected[..self.selected.len() - 1]))
        }
    }

    pub fn is_expanded(&self, path: &[usize]) -> bool {
        self.expanded.iter().any(|p| p.as_slice() == path)
    }

    pub fn toggle_selected(&mut self) {
        if self.selected_node().children.is_empty() {
            return;
        }
        if let Some(i) = self.expanded.iter().position(|p| *p == self.selected) {
            self.expanded.remove(i);
        } else {
            self.expanded.push(self.selected.clone());
        }
    }

    pub fn move_selection(&mut self, down: bool) {
        let rows = self.visible_rows();
        let pos = rows.iter().position(|p| *p == self.selected).unwrap_or(0);
        let next = if down {
            if pos + 1 < rows.len() { pos + 1 } else { pos }
        } else if pos > 0 {
            pos - 1
        } else {
            pos
        };
        if rows.get(next) != rows.get(pos) {
            self.selected = rows[next].clone();
        }
    }

    pub fn visible_rows(&self) -> Vec<Vec<usize>> {
        let mut out = Vec::new();
        self.collect_rows(vec![], &self.dtb.root, &mut out, true);
        out
    }

    fn collect_rows(
        &self,
        path: Vec<usize>,
        node: &Node,
        out: &mut Vec<Vec<usize>>,
        is_root: bool,
    ) {
        out.push(path.clone());
        if is_root || self.is_expanded(&path) {
            for (i, child) in node.children.iter().enumerate() {
                let mut p = path.clone();
                p.push(i);
                self.collect_rows(p, child, out, false);
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => self.human_readable = !self.human_readable,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(false),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(true),
            KeyCode::Enter | KeyCode::Char(' ') => self.toggle_selected(),
            KeyCode::Right => {
                if !self.is_expanded(&self.selected) {
                    self.toggle_selected();
                }
            }
            KeyCode::Left => self.collapse_selected(),
            _ => {}
        }
    }

    fn collapse_selected(&mut self) {
        if self.is_expanded(&self.selected) {
            self.toggle_selected();
        } else if self.selected.len() > 1 {
            self.selected.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;
    use crate::dtb::Header;

    #[test]
    fn tab_toggles_display_mode() {
        let dtb = Dtb {
            header: Header {
                total_size: 0,
                off_dt_struct: 0,
                off_dt_strings: 0,
                off_mem_rsvmap: 0,
                version: 17,
                last_comp_version: 16,
                boot_cpuid_phys: 0,
                size_dt_strings: 0,
                size_dt_struct: Some(0),
            },
            reservations: Vec::new(),
            root: Node {
                name: "/".into(),
                properties: vec![],
                children: vec![],
            },
        };
        let mut app = App::new(dtb);
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);

        assert!(!app.human_readable);
        app.handle_key(tab);
        assert!(app.human_readable);
        app.handle_key(tab);
        assert!(!app.human_readable);
    }
}
