package util

import (
	"os/exec"
)

func InstallHomebrew() error {
	if !commandExists("brew") {
		cmd := exec.Command("/bin/bash", "-c", "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)")
		_, err := cmd.Output()
		return err
	}

	cmd := exec.Command("brew", "--help")
	_, err := cmd.Output()
	return err

}

func InstallDuti() error {
	if !commandExists("duti") {
		cmd := exec.Command("brew", "install", "duti")
		_, err := cmd.Output()
		return err
	}

	cmd := exec.Command("man", "duti")
	_, err := cmd.Output()
	return err
}
