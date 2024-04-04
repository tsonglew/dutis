package pages

import (
	"fmt"

	"github.com/rivo/tview"
	"github.com/tsonglew/dutis/util"
)

func installHomebrew(app *tview.Application, m *tview.Modal, pages *tview.Pages) {
	app.QueueUpdateDraw(func() {
		err := util.InstallHomebrew()
		if err != nil {
			m.SetText(fmt.Sprintf("Failed to install Homebrew. Please try again. Error: %s", err.Error()))
		} else {
			m.SetText("Homebrew installed. Installing Duti...")
			go installDuti(app, m, pages)
		}
	})
}

func installDuti(app *tview.Application, m *tview.Modal, pages *tview.Pages) {
	app.QueueUpdateDraw(func() {
		err := util.InstallDuti()
		if err != nil {
			m.SetText(fmt.Sprintf("Failed to install Duti. Please try again. Error: %s", err.Error()))
		} else {
			pages.SwitchToPage(SuffixPageName)
			_, item := pages.GetFrontPage()
			app.SetFocus(item)
		}
	})
}

func newDepsPage(app *tview.Application, pages *tview.Pages) *TviewPageDef {
	text := "Homebrew and Duti not found. Do you want to install them?"
	btns := []string{"Yes", "Quit"}
	m := tview.NewModal()
	return newTviewPage(
		DepsPageName,
		m.SetText(text).AddButtons(btns).SetDoneFunc(func(buttonIndex int, buttonLabel string) {
			if buttonIndex == 0 {
				m.SetText("Installing Homebrew...").ClearButtons()
				app.ForceDraw()
				go installHomebrew(app, m, pages)
			} else {
				app.Stop()
			}
		}), true)
}
