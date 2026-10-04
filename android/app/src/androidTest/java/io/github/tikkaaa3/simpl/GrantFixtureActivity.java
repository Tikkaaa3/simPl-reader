package io.github.tikkaaa3.simpl;

import android.app.Activity;
import android.os.Bundle;

/** Acts as the storage provider's picker when a test requests URI grants. */
public class GrantFixtureActivity extends Activity {
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        ImportFixtureProvider.grant(this);
        finish();
    }
}
