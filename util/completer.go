package util

import "github.com/c-bata/go-prompt"

func MainCompleter(d prompt.Document) []prompt.Suggest {
	s := []prompt.Suggest{
		{Text: "1", Description: "suffix, change default application by suffix(eg. .txt, .md, .go)"},
		{Text: "2", Description: "preset, change default application by preset(eg. code, office, image)"},
	}
	return prompt.FilterHasPrefix(s, d.GetWordBeforeCursor(), true)
}

func PresetCompleter(d prompt.Document) []prompt.Suggest {
	s := []prompt.Suggest{
		{Text: "code", Description: "For popular coding files"},
		{Text: "text", Description: "For popular text files"},
		{Text: "image", Description: "For popular image files"},
	}
	return prompt.FilterHasPrefix(s, d.GetWordBeforeCursor(), true)
}

func SuffixCompleter(d prompt.Document) []prompt.Suggest {
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
