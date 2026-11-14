import { Navigate, Route, Routes, useSearchParams } from 'react-router-dom'
import Shell from '@/components/Shell'
import Today from '@/pages/Today'
import MapPage from '@/pages/MapPage'
import Thread from '@/pages/Thread'
import Source from '@/pages/Source'
import Search from '@/pages/Search'
import Items from '@/pages/Items'
import Log from '@/pages/Log'
import Graph from '@/pages/Graph'

/** The thread overview map is gone: /map goes to the Graph. A thread's own focus view (notes as cards) is still MapPage. */
function MapRoute() {
  const [sp] = useSearchParams()
  return sp.get('thread') ? <MapPage /> : <Navigate to="/graph" replace />
}

export default function App() {
  return (
    <Shell>
      <Routes>
        <Route path="/" element={<Today />} />
        <Route path="/map" element={<MapRoute />} />
        <Route path="/thread/:name" element={<Thread />} />
        <Route path="/source/:id" element={<Source />} />
        <Route path="/search" element={<Search />} />
        <Route path="/items" element={<Items />} />
        <Route path="/log" element={<Log />} />
        <Route path="/graph" element={<Graph />} />
        <Route path="*" element={<Today />} />
      </Routes>
    </Shell>
  )
}
