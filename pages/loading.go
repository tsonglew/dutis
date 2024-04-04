package pages

import "github.com/rivo/tview"

var loadingModal = tview.NewModal()

func newLoadingUtiPage(app *tview.Application, pages *tview.Pages) *TviewPageDef {
	return newTviewPage(
		LoadingUtiPageName,
		loadingModal,
		false)
}
