use gpui_kit::prelude::*;
use gpui_kit::{
    point, px, AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId,
    LayoutId, Pixels, Window,
};

/// How far under the top of what is in view the element is kept.
const MARGIN: Pixels = px(6.);

/// How far down something at `top` has to move to be under the top of what
/// is in view, which is at `view_top`. Nothing, when it is already there.
fn pushed_down(top: Pixels, view_top: Pixels) -> Pixels {
    (view_top + MARGIN - top).max(px(0.))
}

/// Something that sits where it was laid out, unless that is above what a
/// scrolling list has in view: then it sits at the top of the view instead.
///
/// The bar of actions belongs to the top of its message. A message taller
/// than the window has its top scrolled away while its end is being read,
/// and the bar went with it: the message was under the pointer and there was
/// nothing to press. A list draws its rows inside a mask that is its own
/// bounds, so the top of the mask is the top of what is in view, and nobody
/// has to tell this which list it is in.
pub(super) struct InView {
    element: AnyElement,
}

/// Keeps `element` in view; see [`InView`].
pub(super) fn in_view(element: impl IntoElement) -> InView {
    InView { element: element.into_any_element() }
}

impl IntoElement for InView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for InView {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.element.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let down = pushed_down(bounds.top(), window.content_mask().bounds.top());
        window.with_element_offset(point(px(0.), down), |window| {
            self.element.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.element.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::px;

    use super::{pushed_down, MARGIN};

    #[test]
    fn what_is_in_view_stays_where_it_is() {
        assert_eq!(pushed_down(px(300.), px(80.)), px(0.));
        assert_eq!(pushed_down(px(80.) + MARGIN, px(80.)), px(0.));
    }

    #[test]
    fn what_is_above_the_view_comes_down_to_its_top() {
        // The top of a tall message, six hundred pixels above the list.
        assert_eq!(pushed_down(px(-520.), px(80.)), px(600.) + MARGIN);
        assert_eq!(px(-520.) + pushed_down(px(-520.), px(80.)), px(80.) + MARGIN);
    }
}
