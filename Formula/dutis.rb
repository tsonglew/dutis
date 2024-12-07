class Dutis < Formula
  desc "Command-line tool to manage default applications for file types on macOS"
  homepage "https://github.com/tsonglew/dutis"
  url "https://github.com/tsonglew/dutis/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_ACTUAL_SHA256"
  license "MIT"
  head "https://github.com/tsonglew/dutis.git", branch: "main"

  depends_on "rust" => :build
  depends_on :macos

  def install
    system "cargo", "install", "--root", prefix, "--path", "."
    # Install shell completions
    generate_completions_from_executable(bin/"dutis", "--generate-shell-completion")
  end

  test do
    assert_match "dutis #{version}", shell_output("#{bin}/dutis --version")
  end
end
