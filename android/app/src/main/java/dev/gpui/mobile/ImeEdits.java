package dev.gpui.mobile;

/** Keeps the native editor in sync with the IME's retained editable buffer. */
final class ImeEdits {
    interface Sink {
        void send(int kind, String text, int start, int end);
    }

    private final Sink sink;
    private String previous = "";
    private int markedStart = -1;

    ImeEdits(Sink sink) { this.sink = sink; }

    void reset() {
        previous = "";
        markedStart = -1;
    }

    void sync(String text, int selectionStart, int selectionEnd, int composingStart) {
        // Native commits insert at the cursor or replace the marked range.
        // Finish that range first, putting the native cursor at the end of
        // the known text before applying a replacement or deletion.
        if (markedStart >= 0) {
            sink.send(1, previous.substring(markedStart), 0, 0);
            markedStart = -1;
        }

        int common = 0;
        while (common < previous.length() && common < text.length()
                && previous.charAt(common) == text.charAt(common)) common++;
        if (composingStart >= 0) {
            common = Math.min(common, Math.min(composingStart,
                    Math.min(selectionStart, selectionEnd)));
        }
        // Android offsets are UTF-16. Never leave half an emoji in the prefix.
        if (common > 0 && common < previous.length()
                && Character.isHighSurrogate(previous.charAt(common - 1))
                && Character.isLowSurrogate(previous.charAt(common))) common--;

        int removed = previous.length() - common;
        if (removed > 0) sink.send(2, "", removed, 0);
        String suffix = text.substring(common);
        if (composingStart >= 0) {
            sink.send(0, suffix, selectionStart - common, selectionEnd - common);
            markedStart = common;
        } else if (!suffix.isEmpty()) {
            sink.send(1, suffix, 0, 0);
        }
        previous = text;
    }
}
