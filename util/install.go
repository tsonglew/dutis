package util

import (
	"fmt"
	"os/exec"
)

func InstallDeps() {
	installHomebrew()
	installDuti()
}

func installHomebrew() {
	fmt.Println("Check Homebrew Environment")
	if !commandExists("brew") {
		fmt.Println("Homebrew not exists, installing ...")
		cmd := exec.Command("/bin/bash", "-c", "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)")
		_, _ = cmd.Output()
	}

	cmd := exec.Command("brew", "--help")
	_, err := cmd.Output()

	if err != nil {
		err.Error()
	} else {
		fmt.Print("Homebrew works fine")
	}
}

func installDuti() {
	fmt.Println("Check Duti Environment")
	if !commandExists("duti") {
		fmt.Println("Duti not exists, installing ...")
		cmd := exec.Command("brew", "install", "duti")
		_, err := cmd.Output()
		if err != nil {
			fmt.Println(string(err.(*exec.ExitError).Stderr))
			panic(err)
		}
	}

	cmd := exec.Command("man", "duti")
	_, err := cmd.Output()

	if err != nil {
		fmt.Println(string(err.(*exec.ExitError).Stderr))
		panic(err)
	} else {
		fmt.Println("Duti works fine")
	}
}
