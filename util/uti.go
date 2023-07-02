package util

import (
	"fmt"
	"log"
	"os"
	"os/exec"
	"regexp"
	"sync"
)

type Uti struct {
	Name       string
	Path       string
	Identifier string
}

var kMDItemCFBundleIdentifierPattern = regexp.MustCompile(`kMDItemCFBundleIdentifier\s+=\s+"(.+)"`)

func ListUti(path string) map[string]Uti {
	files, err := os.ReadDir(path)
	r := make(map[string]Uti)
	if err != nil {
		log.Fatal(err)
	}

	c := make(chan Uti)
	wg := &sync.WaitGroup{}
	wg.Add(len(files))

	for _, file := range files {
		go func(file os.DirEntry, wg *sync.WaitGroup) {
			defer wg.Done()

			fp := path + "/" + file.Name()
			cmd := exec.Command("mdls", "-name", "kMDItemCFBundleIdentifier", fp)
			out, err := cmd.Output()
			if err != nil {
				log.Fatal(err)
			}
			match := kMDItemCFBundleIdentifierPattern.FindStringSubmatch(string(out))
			if len(match) > 0 {
				c <- Uti{file.Name(), fp, match[1]}
			}
		}(file, wg)
	}

	go func(group *sync.WaitGroup) {
		wg.Wait()
		close(c)
	}(wg)

	for v := range c {
		r[v.Name] = v
	}
	return r
}

func ListApplicationsUti() map[string]Uti {
	return ListUti("/Applications")
}

func SetDefaultApplication(uti string, suffix string) {
	fmt.Println("Set default application for", suffix, "to", uti)
	cmd := exec.Command("duti", "-s", uti, suffix, "all")
	_, err := cmd.Output()
	if err != nil {
		log.Fatal(err)
	}
}
