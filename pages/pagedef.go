package pages

import "github.com/rivo/tview"

type TviewPageDef struct {
	name            string
	item            tview.Primitive
	resize, visible bool
}

func newTviewPage(name string, item tview.Primitive, visiable bool) *TviewPageDef {
	return &TviewPageDef{name, item, false, visiable}
}
