package main

import (
	"fmt"
	"github.com/c-bata/go-prompt"
	"github.com/tsonglew/dutis/util"
	"strings"
)

var utiMap = util.ListApplicationsUti()

const YouSelectPrompt = "You selected "

func chooseUti() string {
	fmt.Println("Please input uti.(Tab for auto complement)")

	promptHandler := func(d prompt.Document) []prompt.Suggest {
		var p []prompt.Suggest
		for _, v := range utiMap {
			p = append(p, prompt.Suggest{Text: v.Name, Description: "uti: " + v.Identifier})
		}
		return prompt.FilterHasPrefix(p, d.GetWordBeforeCursor(), true)
	}

	t := prompt.Input("> ", promptHandler)
	fmt.Println(YouSelectPrompt + t)
	return t
}

func chooseSuffix() string {
	fmt.Println("Please input suffix.(Tab for auto complement)")
	t := prompt.Input("> ", util.SuffixCompleter)
	fmt.Println(YouSelectPrompt + t)
	return t
}

func choosePreset() {
	fmt.Println("Please input preset.(Tab for auto complement)")
	t := prompt.Input("> ", util.PresetCompleter)
	fmt.Println(YouSelectPrompt + t)
}

func printRecommend(suf string) {
	pmp := strings.Repeat("=", 10) + " Recommended applications " + strings.Repeat("=", 10)
	fmt.Println(pmp)
	if recommendApplications := util.LSCopyAllRoleHandlersForContentType(suf); len(recommendApplications) > 0 {
		for _, n := range util.LSCopyAllRoleHandlersForContentType(suf) {
			fmt.Println(n)
		}
	} else {
		fmt.Println("No recommend applications")
	}
	fmt.Println(strings.Repeat("=", len(pmp)))
}

func main() {
	util.InstallDeps()
	//fmt.Println("Please select mode by number.(Tab for auto complement)\n(1). change default application by suffix\n(2).
	// change default application by preset")
	//t := prompt.Input("> ", mainCompleter)
	//fmt.Println("You selected " + t)
	t := "1"
	var suf string
	switch t {
	case "1":
		suf = chooseSuffix()
	case "2":
		choosePreset()
	}

	printRecommend(suf)

	utiName := chooseUti()
	if utiItem, ok := utiMap[utiName]; ok {
		util.SetDefaultApplication(utiItem.Identifier, suf)
	} else {
		fmt.Printf("uti %s not found\n", utiName)
	}
}
