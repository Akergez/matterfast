use super::host_action::HostAction;

/// The host controls that apply to one participant. Only those: offering
/// "stop sharing" to someone who is not sharing is a button that does
/// nothing.
pub(super) fn host_controls(sharing: bool, hand_up: bool) -> Vec<(HostAction, &'static str)> {
    let mut controls = vec![(HostAction::Mute, "Mute them")];
    if sharing {
        controls.push((HostAction::StopSharing, "Stop their screen share"));
    }
    if hand_up {
        controls.push((HostAction::LowerHand, "Lower their hand"));
    }
    controls.push((HostAction::MakeHost, "Make them the host"));
    controls.push((HostAction::Remove, "Remove from call"));
    controls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_is_only_offered_what_applies() {
        let actions = |sharing, hand| -> Vec<HostAction> {
            host_controls(sharing, hand)
                .into_iter()
                .map(|(action, _)| action)
                .collect()
        };
        assert_eq!(
            actions(false, false),
            [HostAction::Mute, HostAction::MakeHost, HostAction::Remove]
        );
        assert!(actions(true, false).contains(&HostAction::StopSharing));
        assert!(actions(false, true).contains(&HostAction::LowerHand));
        // Removing someone is always last: the one that cannot be undone
        // should not sit where a different button was a moment ago.
        assert_eq!(actions(true, true).last(), Some(&HostAction::Remove));
    }
}
