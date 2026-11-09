use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::app::App;
use crate::dtb::value;

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(f.area());

    let list_area = chunks[0];
    let view_rows = list_area.height as usize;

    let rows = app.visible_rows();
    let sel = rows.iter().position(|r| *r == app.selected).unwrap_or(0);

    let start = if sel < app.offset {
        sel
    } else if sel >= app.offset + view_rows {
        sel.saturating_sub(view_rows.saturating_sub(1))
    } else {
        app.offset
    };

    let mut items: Vec<ListItem> = Vec::new();
    for path in rows.iter().skip(start).take(view_rows) {
        let node = app.node_at(path);
        let indent = "  ".repeat(path.len().saturating_sub(1));
        let prefix = if node.children.is_empty() {
            "  "
        } else if app.is_expanded(path) {
            "▾ "
        } else {
            "▸ "
        };
        let name = format!("{indent}{prefix}{}", node.name);
        let style = if *path == app.selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        items.push(ListItem::new(name).style(style));
    }
    app.offset = start;

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("DTB"));
    f.render_widget(list, list_area);

    let node = app.selected_node();
    let parent = app.selected_parent_node();
    let mut lines = vec![Line::from(Span::styled(
        node.name.clone(),
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    for prop in &node.properties {
        let formatted_value = if app.human_readable {
            value::format_frequency_property(&prop.name, &prop.value, true).or_else(|| {
                match prop.name.as_str() {
                    "reg" => parent.and_then(|parent| {
                        value::format_address_property(
                            &prop.name,
                            &prop.value,
                            cell_count(parent, "#address-cells").unwrap_or(2),
                            2,
                            cell_count(parent, "#size-cells").unwrap_or(1),
                        )
                    }),
                    "ranges" => value::format_address_property(
                        &prop.name,
                        &prop.value,
                        cell_count(node, "#address-cells").unwrap_or(2),
                        parent
                            .and_then(|parent| cell_count(parent, "#address-cells"))
                            .unwrap_or(2),
                        cell_count(node, "#size-cells").unwrap_or(1),
                    ),
                    _ => None,
                }
            })
        } else {
            value::format_frequency_property(&prop.name, &prop.value, false)
        };
        let v = formatted_value
            .map(|text| {
                let kind = if matches!(
                    prop.name.as_str(),
                    "clock-frequency" | "assigned-clock-rates"
                ) {
                    "frequency"
                } else {
                    "address"
                };
                value::ValueFormat { kind, text }
            })
            .unwrap_or_else(|| value::interpret(&prop.name, &prop.value));
        let reference = app
            .phandles
            .resolve_property(prop)
            .map(|paths| format!(" → {}", paths.join(", ")))
            .unwrap_or_default();
        lines.push(Line::from(format!(
            "{} = {}{}   [{}]",
            prop.name, v.text, reference, v.kind
        )));
    }
    if node.children.is_empty() {
        lines.push(Line::from("(leaf node)"));
    }
    let display_mode = if app.human_readable {
        "human-readable"
    } else {
        "raw"
    };
    let detail = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Properties ({display_mode}; TAB to toggle)")),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(detail, chunks[1]);
}

fn cell_count(node: &crate::dtb::Node, name: &str) -> Option<usize> {
    node.properties
        .iter()
        .find(|property| property.name == name)
        .and_then(|property| value::all_cells(&property.value))
        .and_then(|cells| cells.first().copied())
        .map(|cells| cells as usize)
}
