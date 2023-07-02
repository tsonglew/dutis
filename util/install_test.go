package util

import "testing"

func Test_installHomebrew(t *testing.T) {
	tests := []struct {
		name string
	}{
		{"Test_installHomebrew"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			installHomebrew()
		})
	}
}

func Test_installDuti(t *testing.T) {
	tests := []struct {
		name string
	}{
		{"Test_installDuti"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			installDuti()
		})
	}
}
