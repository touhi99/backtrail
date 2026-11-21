import { Navigate, Route, Routes, useSearchParams } from 'react-router-dom'
import Shell from '@/components/Shell'
import Today from '@/pages/Today'
import MapPage from '@/pages/MapPage'
import Thread from '@/pages/Thread'
import Source from '@/pages/Source'
import Graph from '@/pages/Graph'

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
        <Route path="/graph" element={<Graph />} />
        <Route path="*" element={<Today />} />
      </Routes>
    </Shell>
  )
}
