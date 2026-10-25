# Everything CI has, for building and checking this repository on a machine
# that has none of it: Rust with the Android target and cargo-ndk, the Linux
# libraries the toolkit links on a desktop, a JDK, Gradle, and the Android SDK
# and NDK at the versions android/build.gradle.kts names.
#
#   podman build -t localhost/gpui-android-glue-build -f build-aux/build.Containerfile build-aux
#   podman run --rm --userns=keep-id --security-opt label=disable \
#     -e HOME=/tmp -v "$PWD":/src -w /src localhost/gpui-android-glue-build build-aux/check.sh
FROM docker.io/library/rust:1.95-bookworm

RUN apt-get update -q \
 && apt-get install -y -q --no-install-recommends \
      pkg-config \
      libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libvulkan-dev \
      libfontconfig-dev libfreetype-dev \
      openjdk-17-jdk-headless unzip \
 && rm -rf /var/lib/apt/lists/*

RUN rustup target add aarch64-linux-android \
 && cargo install cargo-ndk --locked --version 4.1.2 \
 && rm -rf "$CARGO_HOME/registry"

ENV ANDROID_HOME=/opt/android-sdk
ENV ANDROID_NDK_HOME=/opt/android-sdk/ndk/27.0.12077973
RUN mkdir -p "$ANDROID_HOME/cmdline-tools" \
 && curl -fsSL -o /tmp/tools.zip https://dl.google.com/android/repository/commandlinetools-linux-16111833_latest.zip \
 && unzip -q /tmp/tools.zip -d "$ANDROID_HOME/cmdline-tools" \
 && mv "$ANDROID_HOME/cmdline-tools/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest" \
 && rm /tmp/tools.zip \
 && (yes || true) | "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" --licenses > /dev/null \
 && "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" \
      "platforms;android-36" "build-tools;36.0.0" "ndk;27.0.12077973" > /dev/null

ENV GRADLE_VERSION=9.2.1
RUN curl -fsSL -o /tmp/gradle.zip "https://services.gradle.org/distributions/gradle-$GRADLE_VERSION-bin.zip" \
 && unzip -q /tmp/gradle.zip -d /opt \
 && rm /tmp/gradle.zip
ENV PATH=/opt/gradle-9.2.1/bin:$PATH

# The toolchain rust-toolchain.toml names, so that a run does not fetch it.
RUN rustup toolchain install 1.95 --profile minimal --component clippy --target aarch64-linux-android

# Builds run as the person who started the container (--userns=keep-id):
# everything they write to must be theirs to write.
RUN chmod -R a+rwX "$CARGO_HOME" "$RUSTUP_HOME" "$ANDROID_HOME"
