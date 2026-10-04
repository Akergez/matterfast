/// How many rows a list of people or channels shows.
pub(super) const ROWS: usize = 8;

/// The modifiers the server understands, and what each is for.
pub(super) const MODIFIERS: [(&str, &str); 5] = [
    ("from:", "Messages from a person"),
    ("in:", "Messages in a channel"),
    ("before:", "Messages before a date"),
    ("after:", "Messages after a date"),
    ("on:", "Messages on a date"),
];

/// This application's own: it sends the whole line to the file search
/// instead, so it only means something as the first word.
pub(super) const FILES: (&str, &str) = ("file:", "Search attachments instead");
