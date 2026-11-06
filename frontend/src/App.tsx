import { Header } from './ui/Header'
import { TownCanvas } from './ui/TownCanvas'

export default function App() {
  return (
    <div className="app">
      <Header />
      <main className="app-main">
        <TownCanvas />
      </main>
    </div>
  )
}
