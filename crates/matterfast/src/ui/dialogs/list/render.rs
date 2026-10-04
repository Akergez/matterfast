use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Context, ElementId, FontWeight, Window};

use super::list_body::List;
use super::trailing::Trailing;
use crate::ui::dialogs::callback::run_later;
use crate::ui::kit::{self, Lucide};

impl Render for List {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let mut column = v_flex().gap_4();
        for (section_index, section) in self.sections.iter().enumerate() {
            let mut block = v_flex().gap_1p5();
            if !section.title.is_empty() {
                block = block.child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(section.title.clone()),
                );
            }
            if let Some(search) = &section.search {
                block = block.child(Input::new(search).prefix(Lucide::Search).cleanable(true));
            }

            let mut rows = v_flex()
                .rounded_md()
                .border_1()
                .border_color(theme.border)
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
                let mut line = h_flex()
                    .id(ElementId::Name(
                        format!("row-{section_index}-{}", row.id).into(),
                    ))
                    .w_full()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .items_center()
                    .when(row_index > 0, |line| {
                        line.border_t_1().border_color(theme.border)
                    });
                line = line.child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().truncate().child(row.title.clone()))
                        .when(!row.subtitle.is_empty(), |text| {
                            text.child(
                                div()
                                    .text_xs()
                                    .line_clamp(2)
                                    .text_color(theme.muted_foreground)
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
                            .text_color(theme.muted_foreground)
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
                            Button::new(("button", index))
                                .label(if pressed { done.clone() } else { label.clone() })
                                .small()
                                .primary()
                                .disabled(pressed)
                                .on_click(cx.listener(move |list, _, _, cx| {
                                    list.pressed.insert(key.clone());
                                    cx.notify();
                                    run_later(&action, cx);
                                }))
                                .into_any_element()
                        }
                        Trailing::Icon {
                            icon,
                            tooltip,
                            action,
                        } => {
                            let key = (row.id.clone(), index);
                            let action = action.clone();
                            kit::icon_button(("icon", index), icon.clone(), tooltip.clone())
                                // It only stops a second click on the way to
                                // the server; the row stays until the caller
                                // sends the list back.
                                .disabled(self.pressed.contains(&key))
                                .on_click(cx.listener(move |list, _, _, cx| {
                                    list.pressed.insert(key.clone());
                                    cx.notify();
                                    run_later(&action, cx);
                                }))
                                .into_any_element()
                        }
                    });
                }
                if let Some(action) = row.activate.clone() {
                    line = line
                        .cursor_pointer()
                        .hover(|style| style.bg(theme.list_hover))
                        .on_click(move |_, _, cx| run_later(&action, cx));
                }
                rows = rows.child(line);
            }
            column = column.child(block.child(rows));
        }

        if let (Some(form), Some(action)) = (self.form.clone(), self.form_action.clone()) {
            let submit = form.clone();
            column = column.child(
                v_flex()
                    .gap_1p5()
                    .child(
                        h_flex()
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.form_title.clone()),
                            )
                            .child(Button::new("add").label("Add").small().primary().on_click(
                                move |_, window, cx| {
                                    let values = submit.read(cx).values(cx);
                                    // Ready for the next one.
                                    submit.update(cx, |form, cx| form.clear_texts(window, cx));
                                    let action = action.clone();
                                    cx.defer(move |cx| action(values, cx));
                                },
                            )),
                    )
                    .child(form),
            );
        }

        div()
            .id("list-dialog")
            .max_h(px(520.))
            .overflow_y_scroll()
            .child(column)
    }
}
