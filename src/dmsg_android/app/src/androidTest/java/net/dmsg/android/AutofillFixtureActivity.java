package net.dmsg.android;

import android.app.Activity;
import android.os.Bundle;
import android.text.InputType;
import android.view.View;
import android.view.ViewStructure;
import android.view.autofill.AutofillManager;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.lang.ref.WeakReference;

/** Test APK only, a different UID claiming a website without an association. */
public final class AutofillFixtureActivity extends Activity {
    static WeakReference<AutofillFixtureActivity> current = new WeakReference<>(null);
    EditText password;
    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        current = new WeakReference<>(this);
        LinearLayout layout = new LinearLayout(this) {
            @Override public void onProvideAutofillStructure(ViewStructure structure, int flags) {
                super.onProvideAutofillStructure(structure, flags);
                structure.setClassName("android.webkit.WebView");
                structure.setWebDomain("example.com");
            }
        };
        layout.setOrientation(LinearLayout.VERTICAL); layout.setPadding(40, 80, 40, 40);
        layout.setImportantForAutofill(View.IMPORTANT_FOR_AUTOFILL_YES);
        TextView title = new TextView(this); title.setText("Synthetic untrusted web form"); layout.addView(title);
        EditText username = new EditText(this); username.setId(View.generateViewId()); username.setHint("Username");
        username.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS); username.setAutofillHints(View.AUTOFILL_HINT_USERNAME);
        layout.addView(username, new LinearLayout.LayoutParams(-1, -2));
        password = new EditText(this); password.setId(View.generateViewId()); password.setHint("Password");
        password.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD); password.setAutofillHints(View.AUTOFILL_HINT_PASSWORD);
        layout.addView(password, new LinearLayout.LayoutParams(-1, -2)); setContentView(layout);
        password.requestFocus(); password.postDelayed(() -> getSystemService(AutofillManager.class).requestAutofill(password), 600);
    }
    @Override public void onDestroy() { current.clear(); super.onDestroy(); }
}
