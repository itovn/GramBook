# GramBook cho iOS 5 / 6 (iPad / iPhone / iPod đã jailbreak) — Theos trên Linux, SDK 9.3, armv7 (chạy được A4 trở lên).
# Bộ dựng chép từ GramMusic.
#   make package                    bản thử (-DDEBUG, có công cụ thử Debug/)
#   make package FINALPACKAGE=1     bản phát hành
# Cần lõi Rust đã dựng: tools/build-app.sh (hoặc cargo-ios6.sh build --release -p gm-ffi) → CORE_LIB_DIR/libgm_ffi.a
TARGET := iphone:clang:9.3:5.0
ARCHS = armv7
INSTALL_TARGET_PROCESSES = GramBook

# dpkg của iOS 5 / 6 chỉ đọc .deb nén gzip; nhân iOS 5 / 6 chỉ hiểu chữ ký SHA-1
THEOS_PLATFORM_DEB_COMPRESSION_TYPE = gzip
TARGET_CODESIGN_FLAGS = -S -Hsha1

include $(THEOS)/makefiles/common.mk

APPLICATION_NAME = GramBook
GramBook_FILES = main.m $(wildcard Core/*.m) Core/GMShims.c $(wildcard UI/*.m)
ifneq ($(FINALPACKAGE),1)
GramBook_FILES += $(wildcard Debug/*.m)
endif
GramBook_FRAMEWORKS = UIKit Foundation CoreGraphics QuartzCore ImageIO CoreText CFNetwork Security
# API mới hơn iOS 5.0 dùng thẳng là lỗi lúc dựng (TubeMini D24) — chỗ cần iOS 6 thì hỏi lúc chạy (NSClassFromString…)
GramBook_CFLAGS = -fobjc-arc -Wno-deprecated-declarations -Werror=unguarded-availability -Werror=partial-availability \
	-ICore -IUI -IDebug
# Lõi do tools/build-app.sh dựng ở <thư mục chứa repo>/target-ios (cạnh repo)
CORE_LIB_DIR ?= $(abspath $(CURDIR)/../../target-ios/armv7-apple-ios6/release)
GramBook_LDFLAGS = -L$(CORE_LIB_DIR) -lgm_ffi -Wl,-dead_strip

include $(THEOS_MAKE_PATH)/application.mk
