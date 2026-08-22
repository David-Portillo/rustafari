# Homebrew cask for rustafari.
#
# This file belongs in a tap repository named `homebrew-rustafari`, under
# `Casks/rustafari.rb`. Users then install with:
#
#   brew tap David-Portillo/rustafari
#   brew install --cask rustafari
#
# On each release, bump `version` and replace `sha256` with the DMG's checksum
# from the release's SHA256SUMS file.
cask "rustafari" do
  version "0.6.0"
  sha256 "REPLACE_WITH_DMG_SHA256"

  url "https://github.com/David-Portillo/rustafari/releases/download/v#{version}/rustafari-#{version}-macos.dmg"
  name "rustafari"
  desc "Swiss Army knife devtoy — offline developer utilities in a single native app"
  homepage "https://github.com/David-Portillo/rustafari"

  # Matches LSMinimumSystemVersion in packaging/macos/Info.plist.
  depends_on macos: :big_sur

  app "rustafari.app"

  zap trash: [
    "~/Library/Application Support/rustafari",
    "~/Library/Saved Application State/dev.rustafari.app.savedState",
  ]
end
