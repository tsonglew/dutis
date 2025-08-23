class Dutis < Formula
  desc "macOS Application File Extension Manager"
  homepage "https://github.com/tsonglew/dutis"
  url "https://github.com/tsonglew/dutis/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER_SHA256" # This will need to be updated with actual SHA256
  license "MIT"
  head "https://github.com/tsonglew/dutis.git", branch: "main"

  depends_on "rust" => :build
  depends_on "duti"

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    # Test that the binary can run and show help
    system "#{bin}/dutis", "--help"
  end
end
