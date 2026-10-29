import FlipbookSpike from "./spike/flipbook/FlipbookSpike";

// Pre-M1 spike: dev builds show the flipbook prototype until M1 builds the real reader.
// Release builds keep the placeholder shell, because `tauri build` leaves out the spike's page command.
function App() {
  if (import.meta.env.DEV) return <FlipbookSpike />;
  return (
    <main>
      <h1>Scriptorium</h1>
      <p>Assembling scanned pages into virtual books.</p>
    </main>
  );
}

export default App;
