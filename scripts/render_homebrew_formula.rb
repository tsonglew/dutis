#!/usr/bin/env ruby
# frozen_string_literal: true

version, sha256, url, output_path = ARGV
abort "Usage: #{$PROGRAM_NAME} VERSION SHA256 URL OUTPUT_PATH" unless output_path
abort "Invalid version: #{version}" unless version.match?(/\A\d+\.\d+\.\d+(?:[-.][0-9A-Za-z.-]+)?\z/)
abort "Invalid SHA-256: #{sha256}" unless sha256.match?(/\A[0-9a-f]{64}\z/)
abort "Invalid release URL: #{url}" unless url.start_with?("https://github.com/tsonglew/dutis/releases/download/")

formula = <<~RUBY
  class Dutis < Formula
    desc "Manage default applications for file extensions on macOS"
    homepage "https://github.com/tsonglew/dutis"
    url "#{url}"
    version "#{version}"
    sha256 "#{sha256}"
    license "MIT"

    depends_on :macos
    depends_on "duti"

    def install
      bin.install "dutis"
    end

    test do
      assert_match "Dutis - macOS", shell_output("\#{bin}/dutis --help")
    end
  end
RUBY

File.write(output_path, formula)
