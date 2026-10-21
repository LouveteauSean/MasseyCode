//! The help pane: a one-screen handout for a student who has never seen the
//! MASSEY machine before.
//!
//! The reading order is fixed: what the machine is, the two code views, the
//! editing rules, how to read the diagram, and finally the keys.  The pane is
//! given the width and height it may use, so the type sizes are picked to fit
//! inside that box instead of running off the bottom of a small window.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme;

/// The widest the reading column gets on a normal window; a wider pane gets
/// side margins instead of very long lines.
const MAX_CONTENT_W: f32 = 720.0;
/// A very large window gets a wider column and slightly larger type.
const ROOMY_MIN_W: f32 = 1600.0;
const ROOMY_CONTENT_W: f32 = 1000.0;
/// Body text is never smaller than this, and never larger than the window
/// class allows.
const MIN_BODY: f32 = 8.0;
const MAX_BODY: f32 = 14.5;
const ROOMY_MAX_BODY: f32 = 21.0;
/// The reading column never gets narrower than this, so the fetch chips stay
/// on one row and the key table keeps two columns.
const MIN_CONTENT_W: f32 = 620.0;
/// The column is [`FIT_CHARS`] body sizes wide plus a fixed indent, so the text
/// fills the block instead of leaving a huge blank margin on the right.
const FIT_CHARS: f32 = 40.0;
/// Below these content widths the fetch diagram and the key table wrap into
/// two rows / two columns respectively.
const FETCH_SINGLE_ROW_W: f32 = 620.0;
const KEYS_TWO_COLUMN_W: f32 = 540.0;
/// The assembler form list is only added when the pane is tall enough to keep
/// the body text readable at the same time.
const FORMS_MIN_H: f32 = 700.0;

/// The four transfers that make up one fetch.
const FETCH_STEPS: [(&str, &str); 4] = [
    ("1", "PC -> MAR"),
    ("2", "M[MAR] -> MDR"),
    ("3", "MDR -> IR"),
    ("4", "PC + 1 -> PC"),
];

/// One row per key.  Breakpoints and goto are deliberately absent.
const KEYS: [(&str, &str); 9] = [
    ("F5 / Ctrl+R", "run, one transfer per tick"),
    ("F6", "run, one instruction per tick"),
    ("F11 / F10", "step one transfer / instruction"),
    ("b / Backspace", "back one transfer"),
    ("Ctrl+M", "switch view"),
    ("Ctrl+Shift+R", "reset machine"),
    ("Ctrl+O", "open file"),
    ("? / F1", "help"),
    ("q / Esc", "quit"),
];

/// Type sizes and spacing for one render, chosen so the handout fits `height`.
#[derive(Clone, Copy)]
struct Metrics {
    content_w: f32,
    body: f32,
    line: f32,
    head: f32,
    title: f32,
    pad: f32,
    gap: f32,
    body_rows: f32,
    fetch_single_row: bool,
    key_two_columns: bool,
    show_forms: bool,
}

impl Metrics {
    fn for_pane(width: f32, height: f32) -> Self {
        let roomy = width >= ROOMY_MIN_W;
        let max_body = if roomy { ROOMY_MAX_BODY } else { MAX_BODY };
        let max_content = if roomy {
            ROOMY_CONTENT_W
        } else {
            MAX_CONTENT_W
        };
        let available = (width - 40.0).max(240.0);
        let show_forms = height >= FORMS_MIN_H;

        let mut body = max_body;
        let mut content_w = max_content.min(available);
        for _ in 0..4 {
            let wanted = (body * FIT_CHARS + 120.0)
                .clamp(MIN_CONTENT_W, max_content)
                .min(available);
            let (fetch_single_row, key_two_columns, body_rows) = layout_rows(wanted, show_forms);
            let candidate = Self::fit_body(
                height,
                max_body,
                wanted,
                body_rows,
                fetch_single_row,
                key_two_columns,
                show_forms,
            );
            let settled = (candidate - body).abs() < 0.01 && (wanted - content_w).abs() < 0.01;
            body = candidate;
            content_w = wanted;
            if settled {
                break;
            }
        }

        // One last fit with the settled layout, so the body size always fits.
        let (fetch_single_row, key_two_columns, body_rows) = layout_rows(content_w, show_forms);
        let body = Self::fit_body(
            height,
            max_body,
            content_w,
            body_rows,
            fetch_single_row,
            key_two_columns,
            show_forms,
        );
        Self::at(
            body,
            content_w,
            body_rows,
            fetch_single_row,
            key_two_columns,
            show_forms,
        )
    }

    fn fit_body(
        height: f32,
        max_body: f32,
        content_w: f32,
        body_rows: f32,
        fetch_single_row: bool,
        key_two_columns: bool,
        show_forms: bool,
    ) -> f32 {
        let mut body = max_body;
        while body > MIN_BODY
            && Self::at(
                body,
                content_w,
                body_rows,
                fetch_single_row,
                key_two_columns,
                show_forms,
            )
            .height()
                > height
        {
            body = (body - 0.25).max(MIN_BODY);
        }
        body
    }

    fn at(
        body: f32,
        content_w: f32,
        body_rows: f32,
        fetch_single_row: bool,
        key_two_columns: bool,
        show_forms: bool,
    ) -> Self {
        Self {
            content_w,
            body,
            line: (body * 1.4).round(),
            head: (body * 1.2).round() + 2.0,
            title: (body * 1.5).round() + 6.0,
            pad: (body * 0.95).clamp(7.0, 16.0),
            gap: (body * 0.42).clamp(2.5, 6.0),
            body_rows,
            fetch_single_row,
            key_two_columns,
            show_forms,
        }
    }

    /// The height the whole handout needs: used to pick a body size that fits.
    fn height(self) -> f32 {
        self.pad * 2.0
            + self.title
            + self.head * 5.0
            + self.line * (self.body_rows + 1.0)
            + self.gap * 6.0
    }
}

/// Rows the help uses at a given column width, and whether it gets the wide
/// variant of the fetch diagram and the key table.
fn layout_rows(content_w: f32, show_forms: bool) -> (bool, bool, f32) {
    let fetch_single_row = content_w >= FETCH_SINGLE_ROW_W;
    let key_two_columns = content_w >= KEYS_TWO_COLUMN_W;
    // 3 + fetch + 4 + 3 + 4 + keys, sections 01 to 05, plus the form list.
    let body_rows = 14.0
        + if fetch_single_row { 1.0 } else { 2.0 }
        + if key_two_columns { 5.0 } else { 9.0 }
        + if show_forms { 3.0 } else { 0.0 };
    (fetch_single_row, key_two_columns, body_rows)
}

/// The help handout, rendered into the `width` x `height` box `app.rs` gives it.
pub fn help_pane(width: f32, height: f32) -> AnyElement {
    let m = Metrics::for_pane(width, height);
    // A tall window gets the handout centred like a card instead of a lonely
    // block at the top; a short one starts at the top and clips the overhang.
    let fits = m.height() <= height;
    v_flex()
        .w(px(width))
        .h(px(height))
        .items_center()
        .when(fits, |pane| pane.justify_center())
        .bg(theme::panel())
        .overflow_hidden()
        .child(
            v_flex()
                .w(px(m.content_w))
                .pt(px(m.pad))
                .pb(px(m.pad))
                .child(
                    v_flex()
                        .w_full()
                        .gap(px(m.gap))
                        .child(title_row(m))
                        .child(machine_section(m))
                        .child(views_section(m))
                        .child(editing_section(m))
                        .child(diagram_section(m))
                        .child(keys_section(m))
                        .child(closing_row(m)),
                ),
        )
        .into_any_element()
}

// -- sections --------------------------------------------------------------

fn title_row(m: Metrics) -> AnyElement {
    h_flex()
        .w_full()
        .h(px(m.title))
        .gap(px(9.0))
        .child(
            div()
                .w(px(3.0))
                .h(px(m.body * 1.15))
                .flex_none()
                .rounded(px(2.0))
                .bg(theme::accent()),
        )
        .child(
            div()
                .text_size(px(m.body * 1.28))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::accent())
                .child("MASSEY MACHINE \u{2014} QUICK START"),
        )
        .child(div().flex_1())
        .child(
            div()
                .text_size(px(m.body * 0.8))
                .font_family(theme::mono())
                .text_color(theme::faint())
                .child("HELP"),
        )
        .into_any_element()
}

fn machine_section(m: Metrics) -> AnyElement {
    let rows = [
        "16-bit words. Memory holds 256 words, addressed 00 to FF.",
        "16 registers, R0 to RF. The PC is 8 bits and points at the next word.",
        "Every instruction begins with the same fetch, four transfers long:",
    ];
    let mut section = v_flex()
        .w_full()
        .child(heading("01", "WHAT THE MACHINE IS", m));
    for row in rows {
        section = section.child(bullet_row(row, m));
    }
    section.children(fetch_rows(m)).into_any_element()
}

fn views_section(m: Metrics) -> AnyElement {
    let mut section = v_flex()
        .w_full()
        .child(heading("02", "THE TWO VIEWS", m))
        .child(labeled_row(
            "MACHINE CODE",
            "one word per line, exactly four hex digits.",
            m,
        ))
        .child(labeled_row(
            "ASSEMBLER",
            "only the MASSEY syntax from the PDF notes: one instruction per line,",
            m,
        ))
        .child(continuation_row(
            "hexadecimal operands; no labels, no directives, no comments.",
            m,
        ))
        .child(bullet_row(
            "Line N is address N-1; the gutter shows the hex address.",
            m,
        ));
    if m.show_forms {
        section = section
            .child(forms_row(
                "LOAD Rx,YZ    LOAD Rx,Ry    LOAD Rx,[YZ]    STORE Rx,[YZ]",
                m,
            ))
            .child(forms_row(
                "Rx=Ry+Rz    NEGATE Rx    SHIFT Rx RIGHT|LEFT YZ    HALT",
                m,
            ))
            .child(forms_row("Rx=Ry AND|OR|XOR Rz    JUMP [YZ] IF Rx=R0", m));
    }
    section.into_any_element()
}

/// One mono line of the assembler form list.
fn forms_row(text: &'static str, m: Metrics) -> AnyElement {
    h_flex()
        .w_full()
        .h(px(m.line))
        .items_center()
        .pl(px(m.body * 1.4))
        .child(
            div()
                .text_size(px(m.body * 0.88))
                .font_family(theme::mono())
                .text_color(theme::muted())
                .child(text),
        )
        .into_any_element()
}

fn editing_section(m: Metrics) -> AnyElement {
    v_flex()
        .w_full()
        .child(heading("03", "EDITING", m))
        .child(bullet_row(
            "Both views are editable; data words belong in MACHINE CODE.",
            m,
        ))
        .child(bullet_row(
            "Invalid lines turn red and block the view switch until fixed.",
            m,
        ))
        .child(bullet_row(
            "Adding or removing lines is normal text editing.",
            m,
        ))
        .into_any_element()
}

fn diagram_section(m: Metrics) -> AnyElement {
    v_flex()
        .w_full()
        .child(heading("04", "READING THE DIAGRAM", m))
        .child(bullet_row(
            "CPU box: control unit, ALU, PC, IR, registers; MAR/MDR sit on the bus.",
            m,
        ))
        .child(bullet_row(
            "Lit blocks are the parts in use; CU stays lit for the whole instruction.",
            m,
        ))
        .child(bullet_row(
            "The bus and the addressed MEMORY cell light only for a real read or write.",
            m,
        ))
        .child(bullet_row(
            "The line above the bar names the transfer just done.",
            m,
        ))
        .into_any_element()
}

fn keys_section(m: Metrics) -> AnyElement {
    let mut section = v_flex().w_full().child(heading("05", "KEYS", m));
    if m.key_two_columns {
        let (left, right) = KEYS.split_at(5);
        section = section.child(
            h_flex()
                .w_full()
                .items_start()
                .gap(px(24.0))
                .child(key_column(left, m))
                .child(key_column(right, m)),
        );
    } else {
        section = section.children(KEYS.iter().map(|&(key, action)| key_row(key, action, m)));
    }
    section.into_any_element()
}

fn closing_row(m: Metrics) -> AnyElement {
    h_flex()
        .w_full()
        .h(px(m.line))
        .gap(px(8.0))
        .child(
            div()
                .w(px(6.0))
                .h(px(6.0))
                .flex_none()
                .rounded(px(3.0))
                .bg(theme::accent()),
        )
        .child(
            div()
                .text_size(px(m.body * 0.92))
                .font_family(theme::sans())
                .text_color(theme::muted())
                .child("Press any key to close the help."),
        )
        .into_any_element()
}

// -- rows and blocks -------------------------------------------------------

fn heading(index: &'static str, title: &'static str, m: Metrics) -> AnyElement {
    h_flex()
        .w_full()
        .h(px(m.head))
        .gap(px(8.0))
        .when(index != "01", |row| {
            row.border_t_1().border_color(theme::border_soft())
        })
        .child(
            div()
                .w(px(m.body * 1.3))
                .flex_none()
                .text_size(px(m.body * 0.85))
                .font_family(theme::mono())
                .text_color(theme::faint())
                .child(index),
        )
        .child(
            div()
                .text_size(px(m.body * 1.05))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::muted())
                .child(title),
        )
        .into_any_element()
}

fn bullet_row(text: &'static str, m: Metrics) -> AnyElement {
    h_flex()
        .w_full()
        .h(px(m.line))
        .gap(px(6.0))
        .child(
            div()
                .w(px(m.body * 0.75))
                .flex_none()
                .text_size(px(m.body * 0.9))
                .font_family(theme::mono())
                .text_color(theme::faint())
                .child("\u{203A}"),
        )
        .child(text_cell(text, m.body, theme::text()))
        .into_any_element()
}

fn labeled_row(label: &'static str, text: &'static str, m: Metrics) -> AnyElement {
    h_flex()
        .w_full()
        .h(px(m.line))
        .gap(px(9.0))
        .child(
            div()
                .w(px(label_width(m)))
                .flex_none()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(px(m.body * 0.85))
                .font_family(theme::mono())
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(label),
        )
        .child(text_cell(text, m.body, theme::muted()))
        .into_any_element()
}

fn continuation_row(text: &'static str, m: Metrics) -> AnyElement {
    h_flex()
        .w_full()
        .h(px(m.line))
        .child(div().w(px(label_width(m) + 9.0)).flex_none())
        .child(text_cell(text, m.body, theme::muted()))
        .into_any_element()
}

fn key_row(key: &'static str, action: &'static str, m: Metrics) -> AnyElement {
    h_flex()
        .w_full()
        .h(px(m.line))
        .gap(px(10.0))
        .child(
            div()
                .w(px(key_width(m)))
                .flex_none()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(px(m.body * 0.92))
                .font_family(theme::mono())
                .text_color(theme::text())
                .child(key),
        )
        .child(text_cell(action, m.body * 0.92, theme::muted()))
        .into_any_element()
}

fn key_column(rows: &[(&'static str, &'static str)], m: Metrics) -> AnyElement {
    v_flex()
        .flex_1()
        .min_w_0()
        .children(rows.iter().map(|&(key, action)| key_row(key, action, m)))
        .into_any_element()
}

fn text_cell(text: &'static str, size: f32, color: Hsla) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_size(px(size))
        .font_family(theme::sans())
        .text_color(color)
        .child(text)
        .into_any_element()
}

/// The four fetch transfers, one row of chips when there is room, otherwise
/// two rows of two so the transfer text never has to wrap.
fn fetch_rows(m: Metrics) -> Vec<AnyElement> {
    if m.fetch_single_row {
        vec![chip_row(&FETCH_STEPS, m)]
    } else {
        vec![
            chip_row(&FETCH_STEPS[..2], m),
            chip_row(&FETCH_STEPS[2..], m),
        ]
    }
}

fn chip_row(steps: &[(&'static str, &'static str)], m: Metrics) -> AnyElement {
    h_flex()
        .w_full()
        .h(px(m.line))
        .gap(px(8.0))
        .children(
            steps
                .iter()
                .map(|&(step, transfer)| chip(step, transfer, m)),
        )
        .into_any_element()
}

fn chip(step: &'static str, transfer: &'static str, m: Metrics) -> AnyElement {
    h_flex()
        .flex_1()
        .min_w_0()
        .h(px((m.line * 0.9).max(12.0)))
        .gap(px(7.0))
        .px(px(7.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(theme::border())
        .bg(theme::panel_soft())
        .child(
            div()
                .text_size(px(m.body * 0.78))
                .font_family(theme::mono())
                .text_color(theme::faint())
                .child(step),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(px(m.body * 0.88))
                .font_family(theme::mono())
                .text_color(theme::text())
                .child(transfer),
        )
        .into_any_element()
}

fn label_width(m: Metrics) -> f32 {
    (m.body * 6.6).max(64.0)
}

fn key_width(m: Metrics) -> f32 {
    (m.body * 8.2).clamp(78.0, 176.0)
}

#[cfg(test)]
mod tests {
    use super::Metrics;

    #[test]
    fn handout_fits_the_common_pane_sizes() {
        // app.rs gives help the whole main area: 900x600 -> 900x480,
        // 2560x1528 -> 2560x1408.
        for (width, height) in [
            (900.0, 460.0),
            (900.0, 470.0),
            (1180.0, 640.0),
            (1600.0, 780.0),
            (2560.0, 1380.0),
        ] {
            let metrics = Metrics::for_pane(width, height);
            assert!(
                metrics.height() <= height,
                "{width}x{height} needs {}px at body size {}",
                metrics.height(),
                metrics.body
            );
        }
    }

    #[test]
    fn body_text_stays_readable() {
        // The help screen must stay legible at the smallest supported window.
        let small = Metrics::for_pane(900.0, 470.0);
        assert!(small.body >= 10.9, "900x470 body size {}", small.body);
        let normal = Metrics::for_pane(1180.0, 640.0);
        assert!(normal.body >= 14.0, "1180x640 body size {}", normal.body);
        let large = Metrics::for_pane(2560.0, 1380.0);
        assert!(large.body >= 18.0, "2560x1380 body size {}", large.body);
    }
}
