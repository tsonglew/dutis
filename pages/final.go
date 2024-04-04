package pages

import "github.com/rivo/tview"

var finalModal = tview.NewModal()

func newFinalPage(app *tview.Application) *TviewPageDef {
	return newTviewPage(
		FinalPageName,
		finalModal.
			SetText("Final Page").
			AddButtons([]string{"Quit"}).
			SetDoneFunc(func(buttonIndex int, buttonLabel string) {
				app.Stop()
			}),
		false)
}
