package util

import (
	"fmt"
	"log"
	"os"
	"os/exec"
	"regexp"
	"strings"
	"sync"
)

type Uti struct {
	Name       string
	Path       string
	Identifier string
}

var kMDItemCFBundleIdentifierPattern = regexp.MustCompile(`kMDItemCFBundleIdentifier\s+=\s+"(.+)"`)
var kMDItemContentTypePattern = regexp.MustCompile(`kMDItemContentType\s+=\s+"(.+)"`)

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

func getFileContentType(path string) string {
	cmd := exec.Command("mdls", "-name", "kMDItemContentType", path)
	out, err := cmd.Output()
	if err != nil {
		log.Fatal(err)
	}
	match := kMDItemContentTypePattern.FindStringSubmatch(string(out))
	return match[1]
}

func LSCopyAllRoleHandlersForContentType(suf string) []string {

	contentFile, _ := os.CreateTemp("/tmp", "dutis-content.*"+suf)
	defer func(name string) {
		os.Remove(name)
	}(contentFile.Name())
	contentFileContentType := getFileContentType(contentFile.Name())

	scriptFile, _ := os.CreateTemp("/tmp", "dutis-script.*.swift")
	defer func(name string) {
		os.Remove(name)
	}(scriptFile.Name())

	if _, err := scriptFile.Write([]byte(`
import CoreServices
import Foundation

let args = CommandLine.arguments
guard args.count > 1 else {
    print("Missing argument")
    exit(1)
}

let fileType = args[1]

guard let bundleIds = LSCopyAllRoleHandlersForContentType(fileType as CFString, LSRolesMask.all)  else {
    print("Failed to fetch bundle Ids for specified filetype")
    exit(1)
}

(bundleIds.takeRetainedValue() as NSArray)
    .compactMap { bundleId -> NSArray? in
        guard let retVal = LSCopyApplicationURLsForBundleIdentifier(bundleId as! CFString, nil) else { return nil }
        return retVal.takeRetainedValue() as NSArray
    }
    .flatMap { $0 }
    .forEach { print($0) }
`)); err != nil {
		return []string{}
	}

	cmd := exec.Command("swift", scriptFile.Name(), contentFileContentType)
	out, err := cmd.Output()
	if err != nil {
		return []string{}
	}
	applicationFullPathList := strings.Split(string(out), "\n")
	for i := range applicationFullPathList {
		applicationFullPathList[i] = strings.TrimLeft(applicationFullPathList[i], "file:///Applications/")
	}
	return applicationFullPathList
}
