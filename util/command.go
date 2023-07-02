package util

import (
	"log"
	"os/exec"
)

func commandExists(command string) bool {
	_, err := exec.LookPath(command)
	if err != nil {
		if _, ok := err.(*exec.Error); ok {
			return false
		}
		log.Fatal(err)
	}
	return true
}
