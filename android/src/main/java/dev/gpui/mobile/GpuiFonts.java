package dev.gpui.mobile;

import android.content.Context;
import android.content.res.Configuration;
import android.content.res.Resources;
import android.content.res.TypedArray;
import android.graphics.Paint;
import android.graphics.Typeface;
import android.graphics.fonts.Font;
import android.graphics.text.PositionedGlyphs;
import android.graphics.text.TextRunShaper;
import android.os.Build;
import android.provider.Settings;
import android.util.DisplayMetrics;
import android.util.Log;
import android.util.TypedValue;

import java.io.File;
import java.util.LinkedHashSet;

/**
 * What the system says of its text, for gpui-mobile's text system
 * ({@code src/system_fonts.rs} there reads the answer): how large and how
 * heavy the person wants text, and the files of the font the system draws
 * its own text with.
 *
 * <p>The font is not guessed from a list of names: the system is asked to
 * shape a letter in the device's own text appearance at every weight, and
 * says which file it drew it from. So it is Google Sans on a Pixel, the
 * maker's own font on other phones, and the font the person picked where
 * the maker lets them, as long as it is a file the application may read.
 */
public final class GpuiFonts {
    private static final String TAG = "GpuiFonts";

    private GpuiFonts() {}

    /**
     * Lines of tab-separated fields: {@code scale <factor>},
     * {@code weight <adjustment>}, then {@code face <path> <index>} for each
     * file of the system's font, the regular one first.
     */
    public static String describe(Context context) {
        StringBuilder out = new StringBuilder();
        try {
            Resources resources = context.getResources();
            Configuration config = resources.getConfiguration();
            DisplayMetrics metrics = resources.getDisplayMetrics();
            // Text at body size, in sp over dp: the font size setting, along
            // the curve Android 14 draws large text by.
            float sp = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, 16f, metrics);
            float dp = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, 16f, metrics);
            float scale = dp > 0f ? sp / dp : config.fontScale;
            out.append("scale\t").append(scale).append('\n');
            Typeface system = systemTypeface(context);
            out.append("base\t").append(baseWeight(context, system)).append('\n');
            if (Build.VERSION.SDK_INT >= 31) {
                int adjustment = config.fontWeightAdjustment;
                if (adjustment != Configuration.FONT_WEIGHT_ADJUSTMENT_UNDEFINED) {
                    out.append("weight\t").append(adjustment).append('\n');
                }
            }
            if (Build.VERSION.SDK_INT >= 28) {
                faces(context, out, system);
            }
        } catch (Throwable error) {
            Log.w(TAG, "the system's text could not be read", error);
        }
        String answer = out.toString();
        Log.i(TAG, answer.replace('\n', ' '));
        return answer;
    }

    /**
     * The weight the device draws its own text at. A OnePlus or an Oppo can
     * set it below or above the normal 400; the Typeface carries it, and the
     * text appearance may name it too.
     */
    private static int baseWeight(Context context, Typeface system) {
        // OnePlus and Oppo keep the font weight slider in
        // font_variation_settings: the low 12 bits are the seekbar's value
        // (100..800), shifted by 150 from the wght the font is drawn at, and
        // the 550 default is the normal 400. The high bits say whether the
        // setting is in this format.
        try {
            int packed = Settings.System.getInt(
                    context.getContentResolver(), "font_variation_settings", 550);
            if (packed != 0) {
                int values = packed & 0x0FFF;
                int weight = Math.max(values - 150, 100);
                if (weight >= 100 && weight <= 700) {
                    return weight;
                }
            }
        } catch (Throwable ignored) {
        }
        int weight = 400;
        if (Build.VERSION.SDK_INT >= 28) {
            try {
                int typeface = system.getWeight();
                if (typeface > 0) {
                    weight = typeface;
                }
            } catch (Throwable ignored) {
            }
        }
        Resources resources = context.getResources();
        int[] named = {
            resources.getIdentifier("fontWeight", "attr", "android"),
            resources.getIdentifier("textFontWeight", "attr", "android"),
        };
        if (named[0] == 0 && named[1] == 0) {
            return weight;
        }
        TypedArray attributes = context.obtainStyledAttributes(
                android.R.style.TextAppearance_DeviceDefault, named);
        try {
            int where = attributes.getInt(0, 0);
            if (where <= 0) {
                float weightValue = attributes.getFloat(1, 0f);
                if (weightValue > 0f) {
                    where = Math.round(weightValue);
                }
            }
            // The appearance's word wins when the Typeface itself kept the
            // AOSP normal; an explicit 400 from it does not override a
            // heavier device Typeface.
            if (where > 0 && (weight == 400 || where != 400)) {
                weight = where;
            }
        } finally {
            attributes.recycle();
        }
        return weight;
    }

    /** The files the device's text appearance draws from, regular first. */
    @android.annotation.TargetApi(28)
    private static void faces(Context context, StringBuilder out, Typeface base) {

        int[] weights = { 400, 100, 200, 300, 500, 600, 700, 800, 900 };
        LinkedHashSet<String> files = new LinkedHashSet<>();
        for (boolean italic : new boolean[] { false, true }) {
            for (int weight : weights) {
                Paint paint = new Paint();
                paint.setTypeface(Typeface.create(base, weight, italic));
                PositionedGlyphs glyphs =
                        TextRunShaper.shapeTextRun("a", 0, 1, 0, 1, 0f, 0f, false, paint);
                if (glyphs.glyphCount() == 0) {
                    continue;
                }
                Font font = glyphs.getFont(0);
                File file = font.getFile();
                if (file == null) {
                    continue;
                }
                files.add(file.getAbsolutePath() + "\t" + font.getTtcIndex());
            }
        }
        for (String file : files) {
            out.append("face\t").append(file).append('\n');
        }
    }

    /**
     * The typeface the device draws its own interface with. The text
     * appearance names the application default, which on AOSP is Roboto; a
     * Pixel draws its settings and launcher in Google Sans, a Samsung in
     * SamsungOne, a Xiaomi in MiSans. Where the default is still Roboto, the
     * makers' names are tried in turn and the first one that is a different
     * file is the system's.
     */
    private static Typeface systemTypeface(Context context) {
        Typeface base = Typeface.DEFAULT;
        TypedArray attributes = context.obtainStyledAttributes(
                android.R.style.TextAppearance_DeviceDefault,
                new int[] { android.R.attr.fontFamily });
        try {
            String family = attributes.getString(0);
            if (family != null && !family.isEmpty()) {
                base = Typeface.create(family, Typeface.NORMAL);
            }
        } finally {
            attributes.recycle();
        }

        String baseFile = fileOf(base);
        if (baseFile != null
                && !baseFile.contains("Roboto")
                && !baseFile.contains("DroidSans")) {
            // The person has chosen a font, or the maker has made it the
            // default; it is the system's already.
            return base;
        }
        String[] families = { "google-sans-text", "google-sans", "samsungone",
                "samsung-sans", "misans", "oppo-sans", "oneplus-sans", "harmonyos-sans" };
        for (String family : families) {
            Typeface candidate = Typeface.create(family, Typeface.NORMAL);
            String file = fileOf(candidate);
            if (file != null && !file.equals(baseFile)) {
                return candidate;
            }
        }
        return base;
    }

    /** The file one glyph of {@code typeface} is drawn from, or null. */
    private static String fileOf(Typeface typeface) {
        Paint paint = new Paint();
        paint.setTypeface(typeface);
        PositionedGlyphs glyphs =
                TextRunShaper.shapeTextRun("a", 0, 1, 0, 1, 0f, 0f, false, paint);
        if (glyphs.glyphCount() == 0) {
            return null;
        }
        Font font = glyphs.getFont(0);
        File file = font.getFile();
        return file == null ? null : file.getAbsolutePath();
    }
}
