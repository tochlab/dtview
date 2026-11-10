use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::dtb::{Dtb, Node, phandle::PhandleMap};

#[derive(Debug)]
pub struct App {
    pub dtb: Dtb,
    pub phandles: PhandleMap,
    pub selected: Vec<usize>,
    pub expanded: Vec<Vec<usize>>,
    pub offset: usize,
    pub human_readable: bool,
    pub search_mode: Option<SearchMode>,
    pub search_query: String,
    pub search_results: Vec<SearchResult>,
    pub search_selected: usize,
    pub search_offset: usize,
    pub highlighted_property: Option<(Vec<usize>, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Input,
    Results,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub path: Vec<usize>,
    pub property_index: usize,
    pub property_name: String,
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
            search_mode: None,
            search_query: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            search_offset: 0,
            highlighted_property: None,
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

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.search_mode.is_some() {
            self.handle_search_key(key);
            return false;
        }

        match key.code {
            KeyCode::Char('/') => self.begin_search(),
            KeyCode::Char('q') if key.modifiers == KeyModifiers::NONE => return true,
            KeyCode::Esc => return true,
            KeyCode::Tab => self.human_readable = !self.human_readable,
            KeyCode::Up | KeyCode::Char('k') => {
                self.clear_property_highlight();
                self.move_selection(false);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.clear_property_highlight();
                self.move_selection(true);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.clear_property_highlight();
                self.toggle_selected();
            }
            KeyCode::Right => {
                self.clear_property_highlight();
                if !self.is_expanded(&self.selected) {
                    self.toggle_selected();
                }
            }
            KeyCode::Left => {
                self.clear_property_highlight();
                self.collapse_selected();
            }
            _ => {}
        }
        false
    }

    pub fn node_path(&self, path: &[usize]) -> String {
        if path.is_empty() {
            return "/".into();
        }
        let mut node = &self.dtb.root;
        let mut names = Vec::with_capacity(path.len());
        for &index in path {
            node = &node.children[index];
            names.push(node.name.as_str());
        }
        format!("/{}", names.join("/"))
    }

    fn begin_search(&mut self) {
        self.search_query.clear();
        self.search_results.clear();
        self.search_selected = 0;
        self.search_offset = 0;
        self.search_mode = Some(SearchMode::Input);
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match (self.search_mode, key.code) {
            (_, KeyCode::Esc) => self.search_mode = None,
            (Some(SearchMode::Input), KeyCode::Enter) => {
                self.search_mode = Some(SearchMode::Results);
                self.search_selected = 0;
                self.search_offset = 0;
            }
            (Some(SearchMode::Input), KeyCode::Backspace) => {
                self.search_query.pop();
                self.update_search_results();
            }
            (Some(SearchMode::Input), KeyCode::Char(ch))
                if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.search_query.push(ch);
                self.update_search_results();
            }
            (Some(SearchMode::Results), KeyCode::Up | KeyCode::Char('k')) => {
                self.search_selected = self.search_selected.saturating_sub(1);
            }
            (Some(SearchMode::Results), KeyCode::Down | KeyCode::Char('j')) => {
                if self.search_selected + 1 < self.search_results.len() {
                    self.search_selected += 1;
                }
            }
            (Some(SearchMode::Results), KeyCode::Enter) => self.jump_to_search_result(),
            (Some(SearchMode::Results), KeyCode::Char('/')) => self.begin_search(),
            _ => {}
        }
    }

    fn update_search_results(&mut self) {
        self.search_results.clear();
        self.search_selected = 0;
        self.search_offset = 0;
        if self.search_query.is_empty() {
            return;
        }
        let query = self.search_query.to_lowercase();
        Self::collect_search_results(
            &self.dtb.root,
            &mut Vec::new(),
            &query,
            &mut self.search_results,
        );
    }

    fn collect_search_results(
        node: &Node,
        path: &mut Vec<usize>,
        query: &str,
        results: &mut Vec<SearchResult>,
    ) {
        for (property_index, property) in node.properties.iter().enumerate() {
            if property.name.to_lowercase().contains(query) {
                results.push(SearchResult {
                    path: path.clone(),
                    property_index,
                    property_name: property.name.clone(),
                });
            }
        }
        for (index, child) in node.children.iter().enumerate() {
            path.push(index);
            Self::collect_search_results(child, path, query, results);
            path.pop();
        }
    }

    fn jump_to_search_result(&mut self) {
        let Some(result) = self.search_results.get(self.search_selected).cloned() else {
            return;
        };
        for depth in 0..result.path.len() {
            let ancestor = result.path[..depth].to_vec();
            if !self.is_expanded(&ancestor) {
                self.expanded.push(ancestor);
            }
        }
        self.selected = result.path.clone();
        self.highlighted_property = Some((result.path, result.property_index));
        self.search_mode = None;
    }

    fn clear_property_highlight(&mut self) {
        self.highlighted_property = None;
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

    fn test_dtb() -> Dtb {
        Dtb {
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
                children: vec![Node {
                    name: "soc".into(),
                    properties: vec![crate::dtb::Property {
                        name: "compatible".into(),
                        value: vec![],
                    }],
                    children: vec![Node {
                        name: "uart@0".into(),
                        properties: vec![crate::dtb::Property {
                            name: "clock-frequency".into(),
                            value: vec![],
                        }],
                        children: vec![],
                    }],
                }],
            },
        }
    }

    #[test]
    fn tab_toggles_display_mode() {
        let mut app = App::new(test_dtb());
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);

        assert!(!app.human_readable);
        app.handle_key(tab);
        assert!(app.human_readable);
        app.handle_key(tab);
        assert!(!app.human_readable);
    }

    #[test]
    fn search_finds_properties_and_jumps_to_selected_result() {
        let mut app = App::new(test_dtb());
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT));

        assert_eq!(app.search_results.len(), 2);
        assert_eq!(app.search_results[0].property_name, "compatible");
        assert_eq!(app.search_results[1].property_name, "clock-frequency");

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.selected, vec![0, 0]);
        assert!(app.is_expanded(&[]));
        assert!(app.is_expanded(&[0]));
        assert_eq!(app.highlighted_property, Some((vec![0, 0], 0)));
        assert_eq!(app.search_mode, None);
    }
}
