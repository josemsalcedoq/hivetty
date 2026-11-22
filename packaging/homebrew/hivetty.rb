# Homebrew cask for hivetty (F6-10). A template: `cargo xtask manifests <version> <packages>`
# fills {{version}}, {{dmg}} and {{dmg_sha256}} from a release's packages.
cask "hivetty" do
  version "{{version}}"
  sha256 "{{dmg_sha256}}"

  url "https://github.com/josemsalcedoq/hivetty/releases/download/v#{version}/{{dmg}}"
  name "hivetty"
  desc "GPU terminal for running many coding agents side by side"
  homepage "https://github.com/josemsalcedoq/hivetty"

  app "Hivetty.app"
  binary "#{appdir}/Hivetty.app/Contents/MacOS/hivetty"

  zap trash: "~/Library/Application Support/hivetty"
end
