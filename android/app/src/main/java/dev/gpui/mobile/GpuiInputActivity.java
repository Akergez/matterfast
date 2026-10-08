package dev.gpui.mobile;

import android.app.NativeActivity;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.Bundle;
import android.text.Editable;
import android.text.InputType;
import android.text.Selection;
import android.text.SpannableStringBuilder;
import android.text.TextUtils;
import android.util.Log;
import android.view.KeyEvent;
import android.view.KeyCharacterMap;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.ExtractedText;
import android.view.inputmethod.ExtractedTextRequest;
import android.view.inputmethod.SurroundingText;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputConnectionWrapper;
import android.view.inputmethod.InputMethodManager;

/** NativeActivity with a UI-thread InputConnection for multistage IMEs. */
public class GpuiInputActivity extends NativeActivity {
    /**
     * What the keyboard asks and is told, in the log: there is no other way to
     * see it, and keyboards differ in what they ask. Sizes only, never the text.
     */
    private static final String TAG = "GpuiIme";
    private InputProxy input;
    /** What the platform layer last asked the keyboard to be. */
    private int shownType;
    /** What the application last said the box is for; see gpuiKeyboardHint. */
    private int hint;

    @Override protected void onCreate(Bundle state) {
        // NativeActivity's dlopen alone does not register JNI native methods.
        try {
            String library = getPackageManager().getActivityInfo(getComponentName(),
                    PackageManager.GET_META_DATA).metaData.getString("android.app.lib_name");
            if (library != null) System.loadLibrary(library);
        } catch (PackageManager.NameNotFoundException error) {
            throw new IllegalStateException(error);
        }
        super.onCreate(state);
    }

    public void gpuiShowKeyboard(int keyboardType, long session) {
        runOnUiThread(() -> {
            if (input == null) {
                input = new InputProxy();
                input.setAlpha(0f);
                input.setPadding(0, 0, 0, 0);
                addContentView(input, new ViewGroup.LayoutParams(1, 1));
            }
            // A box that has just been given the keyboard is taken to be at the
            // start of what it says: it is, for a message being begun.
            input.reset(session, true);
            shownType = keyboardType;
            input.setInputType(inputType(keyboardType));
            input.setImeOptions(EditorInfo.IME_FLAG_NO_EXTRACT_UI);
            input.requestFocus();
            InputMethodManager imm = (InputMethodManager) getSystemService(INPUT_METHOD_SERVICE);
            imm.restartInput(input);
            imm.showSoftInput(input, InputMethodManager.SHOW_IMPLICIT);
        });
    }

    /**
     * What the box with the keyboard is for, as the application says it:
     * 0 nothing in particular, 1 a message, 2 an address, 3 a name to sign in
     * with, 4 a password. The platform layer asks for the same plain keyboard
     * whatever has the focus, and only the application knows better.
     *
     * It may be said before the keyboard is asked for or after, so it is
     * kept, and applied at once if the keyboard is already up.
     */
    public void gpuiKeyboardHint(int nextHint) {
        runOnUiThread(() -> {
            Log.d(TAG, "hint: " + hint + " -> " + nextHint);
            if (hint == nextHint) return;
            hint = nextHint;
            if (input == null || !input.hasFocus() || shownType != 0) return;
            input.setInputType(inputType(0));
            ((InputMethodManager) getSystemService(INPUT_METHOD_SERVICE)).restartInput(input);
        });
    }

    /** The widget's type for what the platform layer asked, and the hint with it. */
    private int inputType(int keyboardType) {
        switch (keyboardType) {
            case 1: return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS;
            case 2: return InputType.TYPE_CLASS_PHONE;
            case 3: return InputType.TYPE_CLASS_NUMBER;
            case 4: return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI;
            case 5: return InputType.TYPE_CLASS_NUMBER | InputType.TYPE_NUMBER_FLAG_DECIMAL;
        }
        switch (hint) {
            // Sentences start with a capital and words are corrected. The
            // keyboard decides both from what stands before the cursor; see
            // InputProxy.before.
            case 1: return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_MULTI_LINE
                    | InputType.TYPE_TEXT_FLAG_CAP_SENTENCES | InputType.TYPE_TEXT_FLAG_AUTO_CORRECT;
            case 2: return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI;
            case 3: return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS;
            case 4: return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD;
        }
        return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_MULTI_LINE;
    }

    public void gpuiHideKeyboard(long session) {
        runOnUiThread(() -> {
            if (input == null) return;
            input.reset(session, true);
            InputMethodManager imm = (InputMethodManager) getSystemService(INPUT_METHOD_SERVICE);
            imm.hideSoftInputFromWindow(input.getWindowToken(), 0);
            input.clearFocus();
        });
    }

    public void gpuiResetComposition(long session) {
        runOnUiThread(() -> {
            if (input == null) return;
            // A composition was cut short — the cursor was put somewhere else
            // — and what stands before it now is not known here.
            input.reset(session, false);
            ((InputMethodManager) getSystemService(INPUT_METHOD_SERVICE)).restartInput(input);
        });
    }

    // EditText reports its own selection to the IME when its buffer is
    // cleared. Our buffer holds only the pending composition, so that would
    // falsely report cursor 0 after every commit. Own the notifications here.
    private final class InputProxy extends View {
        private long session;
        private int depth;
        private boolean marked;
        private final Editable text = new SpannableStringBuilder();
        private int type;
        private int options;
        private Integer extractedToken;
        private InputConnection activeConnection;
        /**
         * What has been typed before the cursor since the box got the keyboard.
         * The widget itself is emptied after every word — the text lives in the
         * application — so a keyboard asking it what came before was told
         * "nothing", and could neither tell where a sentence starts nor
         * suggest a word that follows the last one. This is the answer it gets
         * instead. Only the end of it matters, so only the end is kept.
         */
        private final StringBuilder before = new StringBuilder();
        /** Whether {@link #before} starts where the text does. */
        private boolean fromStart = true;
        private static final int BEFORE_KEPT = 512;

        InputProxy() {
            super(GpuiInputActivity.this);
            setFocusable(true);
            setFocusableInTouchMode(true);
            Selection.setSelection(text, 0);
        }

        Editable getText() { return text; }
        int getInputType() { return type; }
        void setInputType(int next) { type = next; }
        void setImeOptions(int next) { options = next; }

        @Override public boolean onCheckIsTextEditor() { return true; }

        @Override public boolean onKeyDown(int code, KeyEvent event) {
            if (activeConnection != null) {
                if (code == KeyEvent.KEYCODE_DEL) {
                    return activeConnection.deleteSurroundingTextInCodePoints(1, 0);
                }
                if (code == KeyEvent.KEYCODE_ENTER) {
                    return activeConnection.commitText("\n", 1);
                }
                int unicode = event.getUnicodeChar();
                if (unicode != 0 && (unicode & KeyCharacterMap.COMBINING_ACCENT) == 0) {
                    return activeConnection.commitText(new String(Character.toChars(unicode)), 1);
                }
            }
            return super.onKeyDown(code, event);
        }

        void reset(long nextSession, boolean atStart) {
            Log.d(TAG, "reset: session=" + nextSession + " atStart=" + atStart);
            depth++;
            getText().clear();
            Selection.setSelection(text, 0);
            marked = false;
            session = nextSession;
            before.setLength(0);
            fromStart = atStart;
            extractedToken = null;
            activeConnection = null;
            depth = 0;
        }

        /** Takes the last {@code count} characters back out of {@link #before}. */
        private void forget(int count) {
            before.setLength(Math.max(0, before.length() - Math.max(0, count)));
            reportState();
        }

        /** The same, counted the way a deletion by code points counts. */
        private void forgetCodePoints(int count) {
            int length = before.length();
            int kept = Math.max(0, before.codePointCount(0, length) - Math.max(0, count));
            before.setLength(before.offsetByCodePoints(0, kept));
            reportState();
        }

        /** Everything known to stand before the cursor, the word being typed included. */
        private CharSequence beforeCursor() {
            Editable text = getText();
            int cursor = Math.max(0, Math.min(Selection.getSelectionStart(text), text.length()));
            return before.toString() + text.subSequence(0, cursor);
        }

        /**
         * The capitals the keyboard should start with. Where the start of the
         * text is not in sight, a letter stands in for it: nothing is known to
         * end a sentence there, so nothing begins one.
         */
        private int capsMode(int requested) {
            String context = (fromStart ? "" : "a") + beforeCursor();
            return TextUtils.getCapsMode(context, context.length(), requested);
        }

        private ExtractedText extractedText() {
            ExtractedText extracted = new ExtractedText();
            extracted.text = before.toString() + getText().toString();
            extracted.startOffset = 0;
            extracted.partialStartOffset = -1;
            extracted.partialEndOffset = -1;
            extracted.selectionStart = before.length() + Math.max(0, Selection.getSelectionStart(text));
            extracted.selectionEnd = before.length() + Math.max(0, Selection.getSelectionEnd(text));
            return extracted;
        }

        private void reportState() {
            InputMethodManager imm = (InputMethodManager) getSystemService(INPUT_METHOD_SERVICE);
            ExtractedText extracted = extractedText();
            int start = BaseInputConnection.getComposingSpanStart(text);
            int end = BaseInputConnection.getComposingSpanEnd(text);
            imm.updateSelection(this, extracted.selectionStart, extracted.selectionEnd,
                    start < 0 ? -1 : before.length() + start,
                    end < 0 ? -1 : before.length() + end);
            if (extractedToken != null) imm.updateExtractedText(this, extractedToken, extracted);
        }

        private void endEdit() {
            if (--depth != 0) return;
            Editable text = getText();
            boolean composing = BaseInputConnection.getComposingSpanStart(text) >= 0;
            if (composing || marked || text.length() > 0) {
                Log.d(TAG, (composing ? "composing " : "commit ") + text.length()
                        + " after " + before.length());
                nativeIme(session, composing ? 0 : 1, text.toString(),
                        Math.max(0, Selection.getSelectionStart(text)),
                        Math.max(0, Selection.getSelectionEnd(text)));
                marked = composing;
                if (!composing) {
                    before.append(text);
                    if (before.length() > BEFORE_KEPT) {
                        before.delete(0, before.length() - BEFORE_KEPT);
                        fromStart = false;
                    }
                    depth++;
                    text.clear();
                    Selection.setSelection(text, 0);
                    depth--;
                }
            }
            reportState();
        }

        @Override public boolean onKeyPreIme(int code, KeyEvent event) {
            if (code == KeyEvent.KEYCODE_BACK && event.getAction() == KeyEvent.ACTION_UP) {
                nativeIme(session, 4, "", 0, 0);
            }
            return super.onKeyPreIme(code, event);
        }

        @Override public InputConnection onCreateInputConnection(EditorInfo info) {
            info.inputType = type;
            info.imeOptions = options;
            InputConnection connection = new BaseInputConnection(this, true) {
                @Override public Editable getEditable() { return InputProxy.this.getText(); }
            };
            // The widget is empty, which reads as the start of a sentence
            // every time the keyboard is restarted.
            info.initialCapsMode = capsMode(info.inputType);
            // A keyboard reads what stands before the cursor in more ways than
            // one — here, by asking for it in pieces, for all of it around the
            // cursor, or for the whole box — and every one of them has to give
            // the same answer. One that still said "nothing" made each letter
            // the first of a sentence.
            CharSequence known = beforeCursor();
            info.initialSelStart = known.length();
            info.initialSelEnd = known.length();
            if (Build.VERSION.SDK_INT >= 30) info.setInitialSurroundingText(known);
            Log.d(TAG, "connection: type=" + info.inputType + " caps=" + info.initialCapsMode
                    + " before=" + known.length() + " fromStart=" + fromStart);
            final long connectionSession = session;
            activeConnection = new InputConnectionWrapper(connection, false) {
                @Override public int getCursorCapsMode(int requested) {
                    if (connectionSession != session) return 0;
                    int caps = capsMode(requested);
                    Log.d(TAG, "getCursorCapsMode -> " + caps);
                    return caps;
                }
                @Override public CharSequence getTextBeforeCursor(int length, int flags) {
                    if (connectionSession != session) return "";
                    CharSequence all = beforeCursor();
                    Log.d(TAG, "getTextBeforeCursor(" + length + ") of " + all.length());
                    return all.subSequence(Math.max(0, all.length() - Math.max(0, length)),
                            all.length());
                }
                @Override public CharSequence getTextAfterCursor(int length, int flags) {
                    if (connectionSession != session) return "";
                    int cursor = Math.max(0, Math.min(Selection.getSelectionEnd(text), text.length()));
                    int count = Math.min(Math.max(0, length), text.length() - cursor);
                    return text.subSequence(cursor, cursor + count).toString();
                }
                @Override public SurroundingText getSurroundingText(int before, int after,
                        int flags) {
                    if (connectionSession != session) return null;
                    CharSequence all = beforeCursor();
                    int from = Math.max(0, all.length() - Math.max(0, before));
                    CharSequence text = all.subSequence(from, all.length());
                    Log.d(TAG, "getSurroundingText(" + before + ") of " + all.length());
                    // Where the piece starts in the whole text is only known
                    // while the start of the text is in sight.
                    return new SurroundingText(text, text.length(), text.length(),
                            fromStart ? from : -1);
                }
                @Override public ExtractedText getExtractedText(ExtractedTextRequest request,
                        int flags) {
                    if (connectionSession != session) return null;
                    if ((flags & InputConnection.GET_EXTRACTED_TEXT_MONITOR) != 0) {
                        extractedToken = request.token;
                    }
                    return extractedText();
                }
                @Override public boolean beginBatchEdit() {
                    if (connectionSession != session) return false;
                    depth++;
                    return true;
                }
                @Override public boolean endBatchEdit() {
                    if (connectionSession != session) return false;
                    if (depth == 0) return false;
                    endEdit();
                    return depth > 0;
                }
                @Override public boolean setComposingText(CharSequence text, int cursor) {
                    if (connectionSession != session) return false;
                    depth++;
                    try { return super.setComposingText(text, cursor); }
                    finally { endEdit(); }
                }
                @Override public boolean setComposingRegion(int start, int end) {
                    if (connectionSession != session) return false;
                    start -= before.length();
                    end -= before.length();
                    if (start < 0 || end < 0 || start > text.length() || end > text.length()) {
                        return false;
                    }
                    depth++;
                    try { return super.setComposingRegion(start, end); }
                    finally { endEdit(); }
                }
                @Override public boolean finishComposingText() {
                    if (connectionSession != session) return false;
                    depth++;
                    try { return super.finishComposingText(); }
                    finally { endEdit(); }
                }
                @Override public boolean commitText(CharSequence text, int cursor) {
                    if (connectionSession != session) return false;
                    depth++;
                    try { return super.commitText(text, cursor); }
                    finally { endEdit(); }
                }
                @Override public boolean setSelection(int start, int end) {
                    if (connectionSession != session) return false;
                    start -= before.length();
                    end -= before.length();
                    if (start < 0 || end < 0 || start > text.length() || end > text.length()) {
                        return false;
                    }
                    depth++;
                    try { return super.setSelection(start, end); }
                    finally { endEdit(); }
                }
                @Override public boolean deleteSurroundingText(int before, int after) {
                    if (connectionSession != session) return false;
                    if (getText().length() == 0 && !marked) {
                        nativeIme(session, 2, "", before, after);
                        forget(before);
                        return true;
                    }
                    depth++;
                    try { return super.deleteSurroundingText(before, after); }
                    finally { endEdit(); }
                }
                @Override public boolean deleteSurroundingTextInCodePoints(int before, int after) {
                    if (connectionSession != session) return false;
                    if (getText().length() == 0 && !marked) {
                        nativeIme(session, 3, "", before, after);
                        forgetCodePoints(before);
                        return true;
                    }
                    depth++;
                    try { return super.deleteSurroundingTextInCodePoints(before, after); }
                    finally { endEdit(); }
                }
                @Override public boolean sendKeyEvent(KeyEvent event) {
                    if (connectionSession != session) return false;
                    if (event.getKeyCode() == KeyEvent.KEYCODE_DEL) {
                        if (event.getAction() == KeyEvent.ACTION_DOWN) deleteSurroundingText(1, 0);
                        return true;
                    }
                    if (event.getKeyCode() == KeyEvent.KEYCODE_ENTER) {
                        if (event.getAction() == KeyEvent.ACTION_DOWN) commitText("\n", 1);
                        return true;
                    }
                    return super.sendKeyEvent(event);
                }
                @Override public boolean performEditorAction(int action) {
                    if (connectionSession != session) return false;
                    if (action == EditorInfo.IME_ACTION_DONE) {
                        finishComposingText();
                        nativeIme(session, 4, "", 0, 0);
                        return true;
                    }
                    return commitText("\n", 1);
                }
            };
            return activeConnection;
        }
    }

    private static native void nativeIme(long session, int kind, String text, int start, int end);
}
