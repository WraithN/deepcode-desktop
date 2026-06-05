import React, { useEffect } from 'react';
import { HashRouter as Router, Routes, Route, Navigate } from 'react-router-dom';
import IntersectObserver from '@/components/common/IntersectObserver';
import { Toaster } from '@/components/ui/sonner';
import { AuthProvider } from '@/contexts/AuthContext';

import { routes } from './routes';

const themeColorMap: Record<string, string> = {
  blue: '213 94% 68%',
  green: '142 71% 45%',
  orange: '25 95% 53%',
  purple: '270 60% 55%',
  pink: '340 75% 55%',
};

const App: React.FC = () => {
  console.log("[App.tsx] Component rendering...");
  
  useEffect(() => {
    console.log("[App.tsx] useEffect running - setting theme...");
    const savedColor = localStorage.getItem('theme_color') || 'blue';
    const hsl = themeColorMap[savedColor] || themeColorMap.blue;
    const root = document.documentElement;
    root.style.setProperty('--primary', hsl);
    root.style.setProperty('--ring', hsl);
    root.style.setProperty('--chart-1', hsl);
    root.style.setProperty('--sidebar-primary', hsl);
    root.style.setProperty('--sidebar-ring', hsl);
    root.style.setProperty('--info', hsl);
    console.log("[App.tsx] Theme set to:", savedColor, hsl);
  }, []);

  console.log("[App.tsx] Routes count:", routes.length);
  console.log("[App.tsx] Route paths:", routes.map(r => r.path));

  return (
    <Router>
      <AuthProvider>
        <IntersectObserver />
        <Routes>
          {routes.map((route, index) => (
            <Route
              key={index}
              path={route.path}
              element={route.element}
            />
          ))}
          <Route path="*" element={<Navigate to="/login" replace />} />
        </Routes>
        <Toaster />
      </AuthProvider>
    </Router>
  );
};

export default App;
