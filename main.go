package main

import (
	"fmt"
	"github.com/c-bata/go-prompt"
	"github.com/tsonglew/dutis/util"
)

var utiMap = util.ListApplicationsUti()

func mainCompleter(d prompt.Document) []prompt.Suggest {
	s := []prompt.Suggest{
		{Text: "1", Description: "suffix, change default application by suffix(eg. .txt, .md, .go)"},
		{Text: "2", Description: "preset, change default application by preset(eg. code, office, image)"},
	}
	return prompt.FilterHasPrefix(s, d.GetWordBeforeCursor(), true)
}

func presetCompleter(d prompt.Document) []prompt.Suggest {
	s := []prompt.Suggest{
		{Text: "code", Description: "For popular coding files"},
		{Text: "text", Description: "For popular text files"},
		{Text: "image", Description: "For popular image files"},
	}
	return prompt.FilterHasPrefix(s, d.GetWordBeforeCursor(), true)
}

func suffixCompleter(d prompt.Document) []prompt.Suggest {
	s := []prompt.Suggest{
		{Text: ".txt", Description: "For text files"},
		{Text: ".md", Description: "For markdown files"},
		{Text: ".go", Description: "For golang files"},
		{Text: ".py", Description: "For python files"},
		{Text: ".js", Description: "For javascript files"},
		{Text: ".ts", Description: "For typescript files"},
		{Text: ".c", Description: "For c files"},
		{Text: ".cpp", Description: "For c++ files"},
		{Text: ".h", Description: "For header files"},
		{Text: ".hpp", Description: "For header files"},
		{Text: ".java", Description: "For java files"},
		{Text: ".sh", Description: "For shell files"},
		{Text: ".zsh", Description: "For zsh files"},
		{Text: ".bash", Description: "For bash files"},
		{Text: ".fish", Description: "For fish files"},
		{Text: ".json", Description: "For json files"},
		{Text: ".xml", Description: "For xml files"},
		{Text: ".html", Description: "For html files"},
		{Text: ".css", Description: "For css files"},
		{Text: ".scss", Description: "For scss files"},
		{Text: ".sass", Description: "For sass files"},
		{Text: ".less", Description: "For less files"},
		{Text: ".vue", Description: "For vue files"},
		{Text: ".tsx", Description: "For typescript files"},
		{Text: ".jsx", Description: "For javascript files"},
		{Text: ".php", Description: "For php files"},
		{Text: ".rb", Description: "For ruby files"},
		{Text: ".rs", Description: "For rust files"},
		{Text: ".swift", Description: "For swift files"},
		{Text: ".kt", Description: "For kotlin files"},
		{Text: ".dart", Description: "For dart files"},
		{Text: ".sql", Description: "For sql files"},
		{Text: ".yml", Description: "For yaml files"},
		{Text: ".yaml", Description: "For yaml files"},
		{Text: ".toml", Description: "For toml files"},
		{Text: ".ini", Description: "For ini files"},
		{Text: ".conf", Description: "For conf files"},
		{Text: ".log", Description: "For log files"},
		{Text: ".csv", Description: "For csv files"},
		{Text: ".tsv", Description: "For tsv files"},
	}
	return prompt.FilterHasPrefix(s, d.GetWordBeforeCursor(), true)
}

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
	fmt.Println("You selected " + t)
	return t
}

func chooseSuffix() string {
	fmt.Println("Please input suffix.(Tab for auto complement)")
	t := prompt.Input("> ", suffixCompleter)
	fmt.Println("You selected " + t)
	return t
}

func choosePreset() {
	fmt.Println("Please input preset.(Tab for auto complement)")
	t := prompt.Input("> ", presetCompleter)
	fmt.Println("You selected " + t)
}

func main() {
	util.InstallDeps()
	//fmt.Println("Please select mode by number.(Tab for auto complement)\n(1). change default application by suffix\n(2). change default application by preset")
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
	utiName := chooseUti()
	if utiItem, ok := utiMap[utiName]; ok {
		util.SetDefaultApplication(utiItem.Identifier, suf)
	} else {
		fmt.Printf("uti %s not found\n", utiName)
	}
}
