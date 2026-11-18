#!/bin/zsh
# Builds the SwiftUI app and bundles it into Backtrail.app. It reads the store through `bt serve` on 127.0.0.1:8799.
set -e
cd "$(dirname "$0")"
swift build -c release
APP=Backtrail.app
rm -rf $APP && mkdir -p $APP/Contents/MacOS $APP/Contents/Resources
cp .build/release/Backtrail $APP/Contents/MacOS/Backtrail
# App icon: the binary renders the artwork, iconutil packs the sizes.
ICON=.build/AppIcon.iconset
rm -rf $ICON && mkdir -p $ICON
BACKTRAIL_ICON=.build/icon-1024.png .build/release/Backtrail
for s in 16 32 128 256 512; do
  sips -z $s $s .build/icon-1024.png --out $ICON/icon_${s}x${s}.png >/dev/null
  sips -z $((s*2)) $((s*2)) .build/icon-1024.png --out $ICON/icon_${s}x${s}@2x.png >/dev/null
done
iconutil -c icns $ICON -o $APP/Contents/Resources/AppIcon.icns
cat > $APP/Contents/Info.plist <<P
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Backtrail</string>
<key>CFBundleIdentifier</key><string>dev.backtrail.mac</string>
<key>CFBundleExecutable</key><string>Backtrail</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>CFBundleVersion</key><string>0.1</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
P
codesign --force --sign - $APP 2>/dev/null || true
echo "built $APP"
