use gpui_adaptive::{prim, ActiveColors as _, Command, Menu, TextField, Toggle};
use gpui_kit::prelude::*;
use gpui_kit::{div, Context, Window};

use super::form::Form;
use super::slot::Slot;

impl Render for Form {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.colors().muted;
        let mut column = prim::column().gap_3();
        for (index, slot) in self.slots.iter().enumerate() {
            column = column.child(match slot {
                Slot::Text { label, input } => {
                    TextField::new(input).label(label.clone()).into_any_element()
                }
                Slot::Switch { label, subtitle, on } => {
                    let entity = cx.entity().downgrade();
                    Toggle::new(("switch", index), label.clone(), *on)
                        .when(!subtitle.is_empty(), |toggle| toggle.description(subtitle.clone()))
                        .on_change(move |on, _, cx| {
                            let _ = entity.update(cx, |form, cx| {
                                if let Some(Slot::Switch { on: held, .. }) = form.slots.get_mut(index) {
                                    *held = on;
                                }
                                cx.notify();
                            });
                        })
                        .into_any_element()
                }
                Slot::Choice {
                    label,
                    options,
                    selected,
                } => {
                    let entity = cx.entity();
                    let menu_options = options.clone();
                    let current = *selected;
                    prim::row()
                        .gap_3()
                        .items_center()
                        .child(div().flex_1().min_w_0().child(label.clone()))
                        .child(
                            Menu::new(Command::new(
                                ("choice", index),
                                options.get(current).map(|(_, label)| label.clone()).unwrap_or_default(),
                            ))
                            .choice()
                            .title(label.clone())
                            .commands(menu_options.iter().enumerate().map(|(option, (_, label))| {
                                let entity = entity.clone();
                                Command::new(("option", option), label.clone())
                                    .checked(option == current)
                                    .on_trigger(move |_, cx| {
                                        entity.update(cx, |form, cx| {
                                            if let Some(Slot::Choice { selected, .. }) = form.slots.get_mut(index) {
                                                *selected = option;
                                            }
                                            cx.notify();
                                        });
                                    })
                            })),
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
