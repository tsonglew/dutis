package pages

import (
	"github.com/rivo/tview"
)

func addTviewPageDefs(pages *tview.Pages, pageDefs ...*TviewPageDef) {
	for _, pageDef := range pageDefs {
		pages.AddPage(pageDef.name, pageDef.item, pageDef.resize, pageDef.visible)
	}
}

func InitPages(app *tview.Application, pages *tview.Pages) {
	addTviewPageDefs(pages, newDepsPage(app, pages), newSuffixPage(app, pages), newLoadingUtiPage(app, pages), newUtiPage(app, pages), newFinalPage(app))
}
