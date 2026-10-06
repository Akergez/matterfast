package dev.gpui.mobile;

import android.app.NativeActivity;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.text.Editable;
import android.text.InputType;
import android.text.Selection;
import android.text.TextUtils;
import android.text.TextWatcher;
import android.view.KeyEvent;
import android.view.ViewGroup;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputConnectionWrapper;
import android.view.inputmethod.InputMethodManager;
import android.widget.EditText;

/** NativeActivity with a UI-thread InputConnection for multistage IMEs. */
public class GpuiInputActivity extends NativeActivity {
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

    private final class InputProxy extends EditText {
        private long session;
        private int depth;
        private boolean marked;
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
            addTextChangedListener(new TextWatcher() {
                public void beforeTextChanged(CharSequence s, int start, int count, int after) {}
                public void onTextChanged(CharSequence s, int start, int before, int count) {}
                public void afterTextChanged(Editable text) {
                    // Hardware keyboards edit the widget directly, outside its
                    // InputConnection. IME mutations are batched by depth below.
                    if (depth == 0) { depth++; endEdit(); }
                }
            });
        }

        @Override public boolean onKeyDown(int code, KeyEvent event) {
            if (code == KeyEvent.KEYCODE_DEL && getText().length() == 0 && !marked) {
                nativeIme(session, 3, "", 1, 0);
                forgetCodePoints(1);
                return true;
            }
            return super.onKeyDown(code, event);
        }

        void reset(long nextSession, boolean atStart) {
            depth++;
            getText().clear();
            marked = false;
            session = nextSession;
            before.setLength(0);
            fromStart = atStart;
            depth = 0;
        }

        /** Takes the last {@code count} characters back out of {@link #before}. */
        private void forget(int count) {
            before.setLength(Math.max(0, before.length() - Math.max(0, count)));
        }

        /** The same, counted the way a deletion by code points counts. */
        private void forgetCodePoints(int count) {
            int length = before.length();
            int kept = Math.max(0, before.codePointCount(0, length) - Math.max(0, count));
            before.setLength(before.offsetByCodePoints(0, kept));
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

        private void endEdit() {
            if (--depth != 0) return;
            Editable text = getText();
            boolean composing = BaseInputConnection.getComposingSpanStart(text) >= 0;
            if (composing || marked || text.length() > 0) {
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
                    depth--;
                }
            }
        }

        @Override public boolean onKeyPreIme(int code, KeyEvent event) {
            if (code == KeyEvent.KEYCODE_BACK && event.getAction() == KeyEvent.ACTION_UP) {
                nativeIme(session, 4, "", 0, 0);
            }
            return super.onKeyPreIme(code, event);
        }

        @Override public InputConnection onCreateInputConnection(EditorInfo info) {
            InputConnection connection = super.onCreateInputConnection(info);
            if (connection == null) return null;
            // The widget is empty, which reads as the start of a sentence
            // every time the keyboard is restarted.
            info.initialCapsMode = capsMode(info.inputType);
            final long connectionSession = session;
            return new InputConnectionWrapper(connection, false) {
                @Override public int getCursorCapsMode(int requested) {
                    if (connectionSession != session) return 0;
                    return capsMode(requested);
                }
                @Override public CharSequence getTextBeforeCursor(int length, int flags) {
                    if (connectionSession != session) return "";
                    CharSequence all = beforeCursor();
                    return all.subSequence(Math.max(0, all.length() - Math.max(0, length)),
                            all.length());
                }
                @Override public boolean beginBatchEdit() {
                    if (connectionSession != session) return false;
                    depth++;
                    return super.beginBatchEdit();
                }
                @Override public boolean endBatchEdit() {
                    if (connectionSession != session) return false;
                    boolean result = super.endBatchEdit();
                    if (depth > 0) endEdit();
                    return result;
                }
                @Override public boolean setComposingText(CharSequence text, int cursor) {
                    if (connectionSession != session) return false;
                    depth++;
                    try { return super.setComposingText(text, cursor); }
                    finally { endEdit(); }
                }
                @Override public boolean setComposingRegion(int start, int end) {
                    if (connectionSession != session) return false;
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
        }
    }

    private static native void nativeIme(long session, int kind, String text, int start, int end);
}
