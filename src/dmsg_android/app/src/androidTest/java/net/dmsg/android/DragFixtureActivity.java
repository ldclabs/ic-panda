package net.dmsg.android;

import android.app.Activity;
import android.content.ClipData;
import android.net.Uri;
import android.os.Bundle;
import android.view.Gravity;
import android.view.View;
import android.widget.TextView;

/** A real second-UID drag source, installed only in the instrumentation APK. */
public final class DragFixtureActivity extends Activity {
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL);
        getWindow().clearFlags(android.view.WindowManager.LayoutParams.FLAG_DIM_BEHIND);
        TextView source = new TextView(this);
        source.setText("Drag sample file");
        source.setContentDescription("Drag sample file");
        source.setGravity(Gravity.CENTER);
        source.setTextSize(18);
        source.setPadding(16, 16, 16, 16);
        Uri uri = Uri.parse("content://" + getPackageName() + ".drag-fixture/sample").buildUpon()
            .appendQueryParameter("sample", getIntent().getStringExtra("sample")).build();
        source.setOnLongClickListener(view -> {
            boolean started = view.startDragAndDrop(
                ClipData.newUri(getContentResolver(), "Sample file", uri),
                new View.DragShadowBuilder(view), null, View.DRAG_FLAG_GLOBAL | View.DRAG_FLAG_GLOBAL_URI_READ);
            android.util.Log.i("DMsgDragFixture", "started=" + started);
            // Android 16 excludes a different task behind this floating Activity
            // from drop targeting. Reveal the recipient while keeping the real
            // global drag and its cross-UID URI grant alive, like switching apps
            // during a drag. Never synthesize an ACTION_DROP or grantUriPermission.
            if (started) view.postDelayed(() -> moveTaskToBack(true), 100);
            return started;
        });
        source.setOnDragListener((view, event) -> {
            if (event.getAction() == android.view.DragEvent.ACTION_DRAG_ENDED) {
                android.util.Log.i("DMsgDragFixture", "accepted=" + event.getResult());
                finish();
            }
            return true;
        });
        setContentView(source);
        getWindow().setGravity(Gravity.TOP | Gravity.END);
        float density = getResources().getDisplayMetrics().density;
        getWindow().setLayout((int)(160 * density), (int)(160 * density));
    }
}
