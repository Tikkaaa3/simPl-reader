package io.github.tikkaaa3.simpl;

import android.media.AudioFormat;
import android.speech.tts.SynthesisCallback;
import android.speech.tts.SynthesisRequest;
import android.speech.tts.TextToSpeech;
import android.speech.tts.TextToSpeechService;
import android.speech.tts.Voice;
import java.util.Collections;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/** Silent PCM with real timing callbacks. Java keeps this standalone test-APK
 * service independent of Kotlin classes shared only by the instrumented app. */
public class TestSpeechEngine extends TextToSpeechService {
    private final AtomicInteger generation = new AtomicInteger();
    @Override protected String[] onGetLanguage() { return new String[] { "eng", "USA", "" }; }
    @Override protected int onIsLanguageAvailable(String lang, String country, String variant) { return TextToSpeech.LANG_COUNTRY_AVAILABLE; }
    @Override protected int onLoadLanguage(String lang, String country, String variant) { return TextToSpeech.LANG_COUNTRY_AVAILABLE; }
    @Override public List<Voice> onGetVoices() {
        return Collections.singletonList(new Voice("p1-test-en", Locale.US, Voice.QUALITY_NORMAL, Voice.LATENCY_NORMAL, false, Collections.emptySet()));
    }
    @Override public String onGetDefaultVoiceNameFor(String lang, String country, String variant) { return "p1-test-en"; }
    @Override public int onIsValidVoiceName(String name) { return "p1-test-en".equals(name) ? TextToSpeech.SUCCESS : TextToSpeech.ERROR; }
    @Override public int onLoadVoice(String name) { return onIsValidVoiceName(name); }
    @Override protected void onStop() { generation.incrementAndGet(); }
    @Override protected void onSynthesizeText(SynthesisRequest request, SynthesisCallback callback) {
        int token = generation.get();
        if (callback.start(16000, AudioFormat.ENCODING_PCM_16BIT, 1) != TextToSpeech.SUCCESS) return;
        int frames = 0;
        Matcher words = Pattern.compile("\\S+").matcher(request.getCharSequenceText());
        while (words.find()) {
            if (generation.get() != token) return;
            callback.rangeStart(frames, words.start(), words.end());
            int count = (int) (1280 * 100f / Math.max(1, request.getSpeechRate()));
            byte[] bytes = new byte[count - count % 2];
            if (callback.audioAvailable(bytes, 0, bytes.length) != TextToSpeech.SUCCESS) return;
            frames += bytes.length / 2;
        }
        // Keep a one-word passage observable through asynchronous UI/service
        // startup without slowing the word clock in long-book follow tests.
        int remaining = Math.max(0, 16000 - frames) * 2;
        byte[] silence = new byte[Math.min(callback.getMaxBufferSize(), remaining)];
        while (remaining > 0) {
            if (generation.get() != token) return;
            int count = Math.min(silence.length, remaining);
            if (callback.audioAvailable(silence, 0, count) != TextToSpeech.SUCCESS) return;
            remaining -= count;
        }
        callback.done();
    }
}
