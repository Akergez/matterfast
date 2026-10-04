/// How many registry entries are drawn at once. The list is not virtual, and
/// nobody reads seven hundred rows: past this, the search box is the way in.
pub(super) const SHOWN: usize = 40;
