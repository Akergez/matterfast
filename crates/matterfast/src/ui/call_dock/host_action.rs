/// What the host can do to somebody else in the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostAction {
    MuteOthers,
    EndCall,
    Mute,
    StopSharing,
    LowerHand,
    Remove,
    MakeHost,
}
