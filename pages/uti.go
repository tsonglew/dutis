package pages

import (
	"github.com/rivo/tview"
)

var utiList = tview.NewList()

type UtiListItem struct {
	text string
	exp  string
}

func newUtiPage(app *tview.Application, pages *tview.Pages) *TviewPageDef {
	return newTviewPage(UtiPageName, utiList, false)
}

func addUtiListItem(app *tview.Application, utiListItems []UtiListItem) {
	utiList.Clear()
	for i, item := range utiListItems {
		var shortcut rune = rune(0)
		if i < 10 {
			shortcut = rune(i + int('1'))
		}
		utiList.AddItem(item.text, item.exp, shortcut, nil)
	}
	utiList.AddItem("Quit", "Quit the application", 'q', func() {
		app.Stop()
	})
	app.SetRoot(utiList, true).SetFocus(utiList)
}
