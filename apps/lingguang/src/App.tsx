import { AnimatePresence, motion } from 'motion/react'
import { useState } from 'react'

import './App.css'
import { useSnfTelemetry } from '@/hooks/useSnfTelemetry'
import { I18nProvider } from '@/i18n'
import { BottomNav, type Tab } from '@/repose/BottomNav'
import { DeviceTab } from '@/repose/DeviceTab'
import { StatusTab, type StatusState } from '@/repose/StatusTab'
import { TrendsTab } from '@/repose/TrendsTab'

function AppContent() {
  const [activeTab, setActiveTab] = useState<Tab>('status')
  const [statusState, setStatusState] = useState<StatusState>('work')
  const [paused, setPaused] = useState(false)
  const telemetry = useSnfTelemetry(paused)

  return (
    <div id="container" className="repose-stage">
      <div className="repose-app-shell">
        <div className="repose-content">
          <AnimatePresence mode="wait">
            <motion.div
              key={activeTab}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.22 }}
              className="repose-tab"
            >
              {activeTab === 'status' && (
                <StatusTab
                  telemetry={telemetry}
                  currentState={statusState}
                  onStateChange={setStatusState}
                />
              )}
              {activeTab === 'trends' && <TrendsTab />}
              {activeTab === 'device' && (
                <DeviceTab
                  telemetry={telemetry}
                  paused={paused}
                  onTogglePaused={() => {
                    setPaused((value) => !value)
                  }}
                />
              )}
            </motion.div>
          </AnimatePresence>
        </div>

        <BottomNav activeTab={activeTab} onTabChange={setActiveTab} />
      </div>
    </div>
  )
}

function App() {
  return (
    <I18nProvider>
      <AppContent />
    </I18nProvider>
  )
}

export default App
