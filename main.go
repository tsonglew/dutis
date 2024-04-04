package main

import (
	"fmt"
	"strings"

	"github.com/c-bata/go-prompt"
	"github.com/rivo/tview"
	"github.com/tsonglew/dutis/pages"
	"github.com/tsonglew/dutis/util"
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
	app := tview.NewApplication()
	apppages := tview.NewPages()
	pages.InitPages(app, apppages)
	if err := app.SetRoot(apppages, true).SetFocus(apppages).Run(); err != nil {
		panic(err)
	}
}
