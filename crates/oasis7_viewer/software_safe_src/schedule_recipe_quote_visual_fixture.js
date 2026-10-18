import { scheduleRecipeQuotes } from "./schedule_recipe_quote_fixture.js";

export function installScheduleRecipeQuoteVisualFixture(fixtures, { core, setFixturePlayerAuth, viewerFixtureBaseSnapshot }) {
  function install(quote) {
    core.injectSnapshot(viewerFixtureBaseSnapshot(), { returnState: false });
    core.applySelection({ kind: "agent", id: "agent-0" });
    setFixturePlayerAuth();
    core.injectScheduleRecipeQuoteForTest(quote);
  }
  fixtures.schedule_recipe_quote = () => install(scheduleRecipeQuotes.low);
  fixtures.schedule_recipe_quote_elevated = () => install(scheduleRecipeQuotes.elevated);
}
