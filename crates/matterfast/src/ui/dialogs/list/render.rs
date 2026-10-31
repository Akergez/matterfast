use gpui_adaptive::prim::{self, Scrolls as _};
use gpui_adaptive::{ActiveColors as _, Command, TextField};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Context, ElementId, FontWeight, Window};

use super::list_body::List;
use super::trailing::Trailing;
use crate::ui::dialogs::callback::run_later;
use crate::ui::kit::{self, Lucide};

impl Render for List {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let mut column = prim::column().gap_4();
        for (section_index, section) in self.sections.iter().enumerate() {
            let mut block = prim::column().gap_1p5();
            if !section.title.is_empty() {
                block = block.child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(section.title.clone()),
                );
            }
            if let Some(search) = &section.search {
                block = block.child(TextField::new(search).icon(Lucide::Search).cleanable());
            }

            let mut rows = prim::column()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .overflow_hidden();
            if section.rows.is_empty() {
                rows = rows.child(match section.empty {
                    Some((icon, title, description)) => div()
                        .h(px(160.))
                        .child(kit::empty_state(icon, title, description, cx))
                        .into_any_element(),
                    None => div().h(px(8.)).into_any_element(),
                });
            }
            for (row_index, row) in section.rows.iter().enumerate() {
                let mut line = prim::row()
                    .id(ElementId::Name(
                        format!("row-{section_index}-{}", row.id).into(),
                    ))
                    .w_full()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .items_center()
                    .when(row_index > 0, |line| {
                        line.border_t_1().border_color(colors.border)
                    });
                line = line.child(
                    prim::column()
                        .flex_1()
                        .min_w_0()
                        .child(div().truncate().child(row.title.clone()))
                        .when(!row.subtitle.is_empty(), |text| {
                            text.child(
                                div()
                                    .text_xs()
                                    .line_clamp(2)
                                    .text_color(colors.muted)
                                    .child(row.subtitle.clone()),
                            )
                        })
                        .when(!row.body.is_empty(), |text| {
                            text.child(div().text_sm().child(row.body.clone()))
                        }),
                );
                for (index, trailing) in row.trailing.iter().enumerate() {
                    line = line.child(match trailing {
                        Trailing::Label(text) => div()
                            .flex_none()
                            .text_sm()
                            .text_color(colors.muted)
                            .child(text.clone())
                            .into_any_element(),
                        Trailing::Button {
                            label,
                            done,
                            action,
                        } => {
                            let key = (row.id.clone(), index);
                            let pressed = self.pressed.contains(&key);
                            let action = action.clone();
                            let list = cx.weak_entity();
                            Command::new(
                                ("button", index),
                                if pressed { done.clone() } else { label.clone() },
                            )
                            .primary()
                            .disabled(pressed)
                            .on_trigger(move |_, cx| {
                                list.update(cx, |list, cx| {
                                    list.pressed.insert(key.clone());
                                    cx.notify();
                                    run_later(&action, cx);
                                })
                                .ok();
                            })
                            .as_button()
                            .into_any_element()
                        }
                        Trailing::Icon {
                            icon,
                            tooltip,
                            action,
                        } => {
                            let key = (row.id.clone(), index);
                            let action = action.clone();
                            kit::icon_button(("icon", index), *icon, tooltip.clone())
                                // It only stops a second click on the way to
                                // the server; the row stays until the caller
                                // sends the list back.
                                .disabled(self.pressed.contains(&key))
                                .on_trigger({
                                    let list = cx.weak_entity();
                                    move |_, cx| {
                                        list.update(cx, |list, cx| {
                                            list.pressed.insert(key.clone());
                                            cx.notify();
                                            run_later(&action, cx);
                                        })
                                        .ok();
                                    }
                                })
                                .as_icon_button()
                                .into_any_element()
                        }
                    });
                }
                if let Some(action) = row.activate.clone() {
                    line = line
                        .cursor_pointer()
                        .hover(|style| style.bg(colors.hover))
                        .on_click(move |_, _, cx| run_later(&action, cx));
                }
                rows = rows.child(line);
            }
            column = column.child(block.child(rows));
        }

        if let (Some(form), Some(action)) = (self.form.clone(), self.form_action.clone()) {
            let submit = form.clone();
            column = column.child(
                prim::column()
                    .gap_1p5()
                    .child(
                        prim::row()
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.form_title.clone()),
                            )
                            .child(Command::new("add", "Add").primary().on_trigger(move |window, cx| {
                                    let values = submit.read(cx).values(cx);
                                    // Ready for the next one.
                                    submit.update(cx, |form, cx| form.clear_texts(window, cx));
                                    let action = action.clone();
                                    cx.defer(move |cx| action(values, cx));
                                },).as_button()),
                    )
                    .child(form),
            );
        }

        div()
            .id("list-dialog")
            .max_h(px(520.))
            .scrolls_y()
            .child(column)
    }
}
