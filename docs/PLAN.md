# gpui-android-glue: план

Общий Android-клей для приложений на GPUI (gpui-kit + gpui-mobile):
matterfast, moneymanager (money-manager-ng), aiddiction. Сейчас этот клей
скопирован в каждое приложение и расходится между копиями.

## 1. Что собираем

### Java (сейчас `android/app/src/main/java/dev/gpui/mobile/` в каждом приложении)

Около 26 классов скопированы из примера gpui-mobile (rev `9075e3a`) байт в байт.
Своё в них только это:

- **IME** (только matterfast): переписанный `GpuiInputActivity`. Это
  `InputProxy extends View` со своим буфером, согласованные ответы на
  `getTextBeforeCursor` / `getSurroundingText` / `getExtractedText` /
  `getCursorCapsMode` (заглавные, автоисправление, подсказки), защита от
  устаревших соединений по `session`, DEL/ENTER/Back/IME_ACTION_DONE.
  `ImeEdits.java` переводит буфер в события `nativeIme`. Тест
  `tests/ImeEditsTest.java` + `build-aux/ime-tests.sh` запускаются на голом JDK.
- **`gpuiKeyboardHint(int)`** (matterfast): назначение поля определяет `InputType`.
- **`GpuiPathPicker.saveCsv`** (moneymanager): `ACTION_CREATE_DOCUMENT`.
  Обобщить до `saveDocument(bytes, mime, name)`.

Java-классы gpui-mobile остаются в пакете `dev.gpui.mobile`: символ
`Java_dev_gpui_mobile_GpuiInputActivity_nativeIme` экспортирует сам gpui-mobile.
Наши новые классы кладём в `dev.gpui.glue`.

### Rust (повторяется в 2–3 приложениях)

| Модуль | Откуда |
|---|---|
| `launch`: `run_android(app, Config, start)`: защита от повторного запуска, logcat, panic hook, `XDG_*` в `internal_data_path`, `rustls-platform-verifier`, deeplink | три `lib.rs` |
| `keyboard`: `Purpose`, `follow()`, `on_tap()`, `keyboard_inset()` | matterfast `keyboard.rs`, aiddiction `ui/mod.rs`, `diffreview-ui/keyboard.rs` |
| `insets`: `system_bars()` | matterfast `shell/system_bars.rs`, moneymanager `shell/chrome.rs`, aiddiction `ui/mod.rs` |
| `appearance`: `system_is_dark()`, `set_status_bar_for(mode)`, `behind_the_system()` | три `appearance.rs` |
| `open_url` (у платформы `cx.open_url` на Android ничего не делает) | matterfast `lib.rs`, aiddiction `ui/mod.rs` |
| `documents`: `save_document()` | moneymanager `android_csv.rs` |
| `chrome`: `title_frame()` | три копии |
| `back`: см. раздел 2 | matterfast `shell_view.rs::close_front` |

На остальных платформах каждая функция либо ничего не делает, либо ведёт
себя как на десктопе, чтобы `#[cfg(target_os = "android")]` из приложений ушли.

## 2. Жест «назад»: выносим

Как сейчас:

- gpui-mobile превращает `AKEYCODE_BACK` в нажатие `escape`
  (`src/android/keyboard.rs:300`) и всегда возвращает `InputStatus::Handled`
  (`src/android/jni.rs:543-579`). Система «назад» не получает никогда:
  с корневого экрана приложение жестом не свернуть.
- При открытой клавиатуре «назад» сначала перехватывает
  `GpuiInputActivity.onKeyPreIme` (`nativeIme` kind 4) и прячет клавиатуру.
- Цепочка «что закрыть» есть только в matterfast: `Shell::close_front`
  (лайтбокс → правая панель → страница беседы → список) плюс `capture_action`
  на `Escape` с исключениями для диалога и списка автодополнения
  (`ui/shell/render.rs:118`). В aiddiction и moneymanager обработки нет:
  закрываются только диалоги gpui-kit, а дальше «назад» ничего не делает.
- **Перестанет работать:** с `targetSdk 36` на Android 16 включается
  predictive back, и `KEYCODE_BACK` больше не доставляется. Сейчас у всех
  `targetSdk = 34`, но Play требует поднимать targetSdk, так что придётся перейти
  на `OnBackInvokedCallback` / `OnBackPressedCallback`.

Модель для библиотеки (как `OnBackPressedCallback.isEnabled` в AndroidX):

- Rust: стек слоёв `back::push(window, cx, handler) -> Guard`. Слой (диалог,
  панель, страница, лайтбокс) регистрируется, пока он открыт, и снимается при
  drop `Guard`. «Назад» отдаётся верхнему слою.
- Как только стек становится пустым или непустым, Rust сообщает Java
  `setBackEnabled(bool)`.
- Java (`dev.gpui.glue.GpuiBack`, подключается из `GpuiActivity.onCreate`):
  колбэк включён, пока стек непуст. Тогда «назад» → `nativeBack()` → канал →
  главный поток GPUI → верхний слой. Когда стек пуст, колбэк выключен, и система
  сама анимирует predictive back и сворачивает приложение. Синхронный ответ из
  Rust не нужен: состояние «хочу назад» Java знает заранее.
- gpui-mobile больше не превращает Back в `escape`: патч в upstream, а до его
  принятия Java-колбэк перехватывает «назад» раньше NativeActivity.
- На десктопе тот же стек вешается на `Escape`. В matterfast `close_front`
  разбивается на слои.
- Клавиатуру «назад» по-прежнему прячет IME. Это идёт раньше колбэков и в стек
  не попадает.

## 3. Устройство репозитория

```
gpui-android-glue/
├── Cargo.toml                 # crate gpui-android-glue (gpui-pre-mobile @ та же rev, gpui-kit, jni)
├── src/{launch,keyboard,insets,appearance,open_url,documents,chrome,back}.rs
├── android/                   # Gradle: com.android.library + maven-publish → AAR
│   ├── settings.gradle.kts
│   ├── build.gradle.kts
│   └── src/main/
│       ├── AndroidManifest.xml          # GpuiPathPicker, GpuiPermissionActivity… (мёрджатся в app)
│       └── java/dev/gpui/{mobile,glue}/
├── android/src/test/java/…/ImeEditsTest.java
├── build-aux/{build-android.sh, android-clang.sh, ime-tests.sh, check-version.sh}
└── .github/workflows/{ci.yml, release.yml}
```

Rust-часть отдельной нативной библиотеки не даёт: она линкуется в cdylib
приложения. Поэтому в AAR только Java и манифест, без `jniLibs`. Этим
публикация проще, чем в tresse-kmp, где для каждой платформы есть свой native-job.

## 4. Публикация (по образцу tresse-kmp)

Одна версия на обе половины: тег `vX.Y.Z` = `version` в `Cargo.toml` =
версия артефакта Maven. Это проверяет `build-aux/check-version.sh`.

- **Rust**: релиз — это git-тег. Приложения пишут
  `gpui-android-glue = { git = "https://github.com/Akergez/gpui-android-glue", tag = "vX.Y.Z" }`.
  На crates.io не публикуем: там нельзя git-зависимости, а gpui-pre-mobile
  есть только в git.
- **Java**: GitHub Packages (Maven) как в tresse-kmp, координаты
  `com.akergez.gpui:android-glue:X.Y.Z`.

`android/build.gradle.kts` (то же, что в tresse-kmp, без KMP):

```kotlin
plugins {
    id("com.android.library") version "8.13.2"
    `maven-publish`
}

group = "com.akergez.gpui"
version = providers.gradleProperty("glueVersion")
    .orElse(
        providers.environmentVariable("GITHUB_REF_TYPE")
            .filter { it == "tag" }
            .flatMap { providers.environmentVariable("GITHUB_REF_NAME") }
            .map { it.removePrefix("v") }
    )
    .orElse("0.1.0-SNAPSHOT")
    .get()

android {
    namespace = "dev.gpui.glue"
    compileSdk = 36
    defaultConfig { minSdk = 26 }
    publishing { singleVariant("release") { withSourcesJar() } }
}

dependencies {
    // То, что раньше каждое приложение подключало «для Java-классов из gpui-mobile».
    api("androidx.core:core:1.12.0")
    api("androidx.core:core-splashscreen:1.0.1")
    api("androidx.biometric:biometric:1.1.0")
    api("androidx.media:media:1.7.1")
}

publishing {
    publications {
        create<MavenPublication>("release") {
            artifactId = "android-glue"
            afterEvaluate { from(components["release"]) }
        }
    }
    repositories {
        val githubRepository = providers.gradleProperty("githubRepository")
            .orElse(providers.environmentVariable("GITHUB_REPOSITORY"))
            .orNull
        if (githubRepository != null) {
            maven {
                name = "GitHubPackages"
                url = uri("https://maven.pkg.github.com/${githubRepository.lowercase()}")
                credentials {
                    username = providers.environmentVariable("GITHUB_ACTOR").orNull
                    password = providers.environmentVariable("GITHUB_TOKEN").orNull
                }
            }
        }
    }
}
```

`.github/workflows/release.yml`:

```yaml
name: release
on:
  push:
    tags: ["v[0-9]+.[0-9]+.[0-9]+", "v[0-9]+.[0-9]+.[0-9]+-*"]
permissions:
  contents: read
env:
  GRADLE_VERSION: "9.2.1"
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - run: build-aux/check-version.sh "${GITHUB_REF_NAME#v}"
      - run: cargo test
      - run: bash build-aux/ime-tests.sh
  publish-maven:
    needs: check
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v5
      - uses: actions/setup-java@v5
        with: { distribution: temurin, java-version: "21" }
      - uses: gradle/actions/setup-gradle@v4
        with: { gradle-version: "${{ env.GRADLE_VERSION }}" }
      - name: Install Android SDK components
        run: |
          export PATH="$PATH:$ANDROID_HOME/cmdline-tools/latest/bin"
          yes | sdkmanager --licenses || true
          sdkmanager "platforms;android-36" "build-tools;36.0.0"
      - name: Publish
        working-directory: android
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: gradle --no-daemon publishReleasePublicationToGitHubPackagesRepository
```

`ci.yml` (каждый push): `cargo test`, `cargo clippy`, сборка под
`aarch64-linux-android` через `cargo ndk`, `ime-tests.sh`,
`gradle assembleRelease`.

### Подключение в приложении

`android/settings.gradle.kts`:

```kotlin
dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
        maven {
            url = uri("https://maven.pkg.github.com/akergez/gpui-android-glue")
            credentials {
                username = providers.environmentVariable("GITHUB_ACTOR").orNull
                    ?: providers.gradleProperty("gpr.user").orNull
                password = providers.environmentVariable("GITHUB_TOKEN").orNull
                    ?: providers.gradleProperty("gpr.key").orNull
            }
        }
    }
}
// Локальная разработка библиотеки: includeBuild подменяет артефакт по group:name.
if (providers.gradleProperty("glueLocal").isPresent) includeBuild("../../gpui-android-glue/android")
```

`app/build.gradle.kts`: `implementation("com.akergez.gpui:android-glue:X.Y.Z")`
вместо четырёх androidx-строк. Копии `dev/gpui/mobile/*.java` из `app/`
удаляются, иначе будет duplicate class.

Учесть: GitHub Packages требует токен даже для чтения публичных пакетов.
Локально нужен PAT с `read:packages` в `~/.gradle/gradle.properties`
(`gpr.user`/`gpr.key`). В CI приложений хватает `GITHUB_TOKEN`, если в
настройках пакета (Manage Actions access) выдать доступ репозиториям
matterfast, moneymanager и aiddiction.

## 5. Порядок работ

1. Каркас репозитория, Java из matterfast (IME + тест), Rust-модули без `back`.
   CI и первый тег `v0.1.0`.
2. Перевести aiddiction (меньше всего своего), потом moneymanager (`save_document`).
3. `back`: стек слоёв + `GpuiBack` на `OnBackInvokedCallback`. Проверить на
   Android 16 с `targetSdk 36`.
4. Перевести matterfast (`close_front` → слои, `keyboard.rs` → библиотека).
5. Предложить в upstream gpui-mobile: IME, `gpuiKeyboardHint`, отказ от
   Back→`escape`.

## 6. Состояние

Шаг 1 сделан: крейт и Android-библиотека в этом репозитории.

- `cargo test` и `clippy -D warnings` проходят на десктопе и под
  `aarch64-linux-android` (с `platform-verifier` и без), `ime-tests.sh`
  проходит, `gradle assembleRelease` собирает AAR, задача
  `publishReleasePublicationToGitHubPackagesRepository` есть, а
  `publishToMavenLocal` даёт `.aar`, `.pom` и `.module`. Всё это запускалось
  в `build-aux/build.Containerfile`.
- Ни разу не запускалось на телефоне. Не проверены: «назад» (ни клавиша, ни
  жест predictive back), `gpuiKeyboardHint` и `saveDocument` в приложении,
  кроме matterfast, где они уже были.
- Репозиторий на GitHub ещё не создан, тега нет, ни одно приложение пока
  на библиотеку не переведено.
