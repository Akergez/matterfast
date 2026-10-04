use super::host_action::HostAction;
use crate::ui::kit::Lucide;

pub(super) fn host_icon(action: HostAction) -> Lucide {
    match action {
        HostAction::Mute | HostAction::MuteOthers => Lucide::MicOff,
        HostAction::StopSharing => Lucide::MonitorOff,
        HostAction::LowerHand => Lucide::Hand,
        HostAction::MakeHost => Lucide::Crown,
        HostAction::Remove => Lucide::UserMinus,
        HostAction::EndCall => Lucide::X,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_host_action_has_an_icon_of_its_own_kind() {
        // Muting one and muting all are the same act on a different number of
        // people, and nothing else may borrow the microphone.
        for action in [
            HostAction::StopSharing,
            HostAction::LowerHand,
            HostAction::MakeHost,
            HostAction::Remove,
            HostAction::EndCall,
        ] {
            assert_ne!(
                host_icon(action).path(),
                host_icon(HostAction::Mute).path(),
                "{action:?}"
            );
        }
    }
}
