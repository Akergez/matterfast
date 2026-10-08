package dev.gpui.mobile;

/** Run with javac/java; exercises the same native-event contract as text_input.rs. */
public final class ImeEditsTest {
    private static final class Editor implements ImeEdits.Sink {
        final StringBuilder text = new StringBuilder();
        int start;
        int end;
        int markedStart = -1;
        int markedEnd;
        int events;

        @Override public void send(int kind, String value, int a, int b) {
            events++;
            if (kind == 2) {
                int from = Math.max(0, start - a);
                int to = Math.min(text.length(), end + b);
                text.delete(from, to);
                start = end = from;
                return;
            }
            int from = markedStart >= 0 ? markedStart : start;
            int to = markedStart >= 0 ? markedEnd : end;
            text.replace(from, to, value);
            if (kind == 0) {
                markedStart = from;
                markedEnd = from + value.length();
                start = from + a;
                end = from + b;
            } else {
                markedStart = -1;
                start = end = from + value.length();
            }
        }
    }

    private static void expect(String expected, Editor editor) {
        if (!expected.equals(editor.text.toString())) {
            throw new AssertionError("Expected " + expected + ", got " + editor.text);
        }
    }

    public static void main(String[] args) {
        Editor editor = new Editor();
        ImeEdits edits = new ImeEdits(editor);
        // A suggestion replaces the composing word, rather than appending it.
        edits.sync("п", 1, 1, 0);
        edits.sync("при", 3, 3, 0);
        edits.sync("привет", 6, 6, -1);
        expect("привет", editor);
        int sent = editor.events;
        edits.sync("привет", 6, 6, -1);
        if (editor.events != sent) throw new AssertionError("Repeated finish committed twice");

        // Reopen an already committed word for autocorrection.
        edits.sync("привет", 6, 6, 0);
        expect("привет", editor);
        edits.sync("приветы", 7, 7, -1);
        expect("приветы", editor);

        // Replace only the last word; preserve the earlier text.
        edits.sync("приветы мир", 11, 11, 8);
        edits.sync("приветы миру", 12, 12, -1);
        expect("приветы миру", editor);

        // A replacement can have text after it, with the cursor inside the word.
        edits.sync("привет миру", 3, 3, 0);
        expect("привет миру", editor);
        edits.sync("привет миру", 6, 6, -1);
        expect("привет миру", editor);

        // Delete a committed word and then insert a shorter replacement.
        edits.sync("привет ", 7, 7, -1);
        edits.sync("привет всем", 11, 11, -1);
        expect("привет всем", editor);

        // Replacing emoji with a shared high surrogate must delete both units.
        edits.sync("привет 😀", 9, 9, -1);
        edits.sync("привет 😁", 9, 9, -1);
        expect("привет 😁", editor);

        // A new focus/session has its own editor and text history.
        edits.reset();
        editor.text.setLength(0);
        editor.start = editor.end = 0;
        edits.sync("новое", 5, 5, -1);
        expect("новое", editor);
        System.out.println("IME replacement, composition, deletion and emoji checks passed");
    }
}
