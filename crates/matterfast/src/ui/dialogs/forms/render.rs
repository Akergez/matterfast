use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, Context, FontWeight, Window};

use super::form::Form;
use super::slot::Slot;

impl Render for Form {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let mut column = v_flex().gap_3();
        for (index, slot) in self.slots.iter().enumerate() {
            column = column.child(match slot {
                Slot::Text { label, input } => v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label.clone()))
                    .child(Input::new(input))
                    .into_any_element(),
                Slot::Switch {
                    label,
                    subtitle,
                    on,
                } => h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().child(label.clone()))
                            .when(!subtitle.is_empty(), |column| {
                                column.child(
                                    div().text_xs().text_color(muted).child(subtitle.clone()),
                                )
                            }),
                    )
                    .child(
                        Switch::new(("switch", index))
                            .checked(*on)
                            .on_click(cx.listener(move |form, on: &bool, _, cx| {
                                if let Some(Slot::Switch { on: held, .. }) =
                                    form.slots.get_mut(index)
                                {
                                    *held = *on;
                                }
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
                Slot::Choice {
                    label,
                    options,
                    selected,
                } => {
                    let entity = cx.entity();
                    let menu_options = options.clone();
                    let current = *selected;
                    h_flex()
                        .gap_3()
                        .items_center()
                        .child(div().flex_1().min_w_0().child(label.clone()))
                        .child(
                            Button::new(("choice", index))
                                .label(
                                    options
                                        .get(current)
                                        .map(|(_, label)| label.clone())
                                        .unwrap_or_default(),
                                )
                                .small()
                                .dropdown_caret(true)
                                .dropdown_menu(move |mut menu, _, _| {
                                    for (option, (_, label)) in menu_options.iter().enumerate() {
                                        let entity = entity.clone();
                                        menu = menu.item(
                                            PopupMenuItem::new(label.clone())
                                                .checked(option == current)
                                                .on_click(move |_, _, cx| {
                                                    entity.update(cx, |form, cx| {
                                                        if let Some(Slot::Choice {
                                                            selected, ..
                                                        }) = form.slots.get_mut(index)
                                                        {
                                                            *selected = option;
                                                        }
                                                        cx.notify();
                                                    });
                                                }),
                                        );
                                    }
                                    menu
                                }),
                        )
                        .into_any_element()
                }
            });
        }
        if !self.note.is_empty() {
            column = column.child(div().text_xs().text_color(muted).child(self.note.clone()));
        }
        column
    }
}
