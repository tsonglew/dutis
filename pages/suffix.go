package pages

import (
	"fmt"
	"strings"

	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"
	"github.com/tsonglew/dutis/util"
)

var words = []string{
	".txt",
	".md",
	".go",
	".py",
	".js",
	".ts",
	".c",
	".cpp",
	".h",
	".hpp",
	".java",
	".sh",
	".zsh",
	".bash",
	".fish",
	".json",
	".xml",
	".html",
	".css",
	".scss",
	".sass",
	".less",
	".vue",
	".tsx",
	".jsx",
	".php",
	".rb",
	".rs",
	".swift",
	".kt",
	".dart",
	".sql",
	".yml",
	".yaml",
	".toml",
	".ini",
	".conf",
	".log",
	".csv",
	".tsv",
}

func autocompelete(currentText string) (entries []string) {
	if len(currentText) == 0 {
		return
	}
	for _, word := range words {
		if strings.Contains(strings.ToLower(word), strings.ToLower(currentText)) {
			entries = append(entries, word)
		}
	}
	if len(entries) <= 1 {
		entries = words
	}
	return
}

func autocompeleted(text string, index, source int) bool {
	if source != tview.AutocompletedNavigate {
		fmt.Println(text)
	}
	return source == tview.AutocompletedEnter || source == tview.AutocompletedClick
}

func afterChosenSuffix(inputField *tview.InputField, app *tview.Application, pages *tview.Pages) {
	suf := inputField.GetText()
	go func() {
		app.QueueUpdateDraw(func() {
			pages.SwitchToPage(LoadingUtiPageName)
			loadingModal.SetText(fmt.Sprintf("Loading recommended applications for %s...", suf))
			go func() {
				if recommendApplications := util.LSCopyAllRoleHandlersForContentType(suf); len(recommendApplications) > 0 {
					utiMap := util.ListApplicationsUti()
					candUtis := make([]UtiListItem, 0, len(recommendApplications))
					for _, n := range util.LSCopyAllRoleHandlersForContentType(suf) {
						if strings.Trim(n, " ") == "" {
							continue
						}
						if uti, ok := utiMap[n]; ok {
							candUtis = append(candUtis, UtiListItem{
								text: n,
								exp:  uti.Identifier,
							})
						}
					}
					app.QueueUpdateDraw(func() {
						addUtiListItem(app, candUtis)
						pages.SwitchToPage(UtiPageName)
					})
				} else {
					app.QueueUpdateDraw(func() {
						finalModal.SetText("No recommend applications")
						pages.SwitchToPage(FinalPageName)
					})
				}

			}()
		})
	}()
}

func newSuffixPage(app *tview.Application, pages *tview.Pages) *TviewPageDef {
	inputField := tview.NewInputField().SetLabel("Please input suffix: ").SetFieldWidth(10)
	inputField.SetDoneFunc(func(key tcell.Key) {
		afterChosenSuffix(inputField, app, pages)
	})
	inputField.SetAutocompleteFunc(autocompelete)
	inputField.SetAutocompletedFunc(autocompeleted)
	return newTviewPage(SuffixPageName, inputField, false)
}
