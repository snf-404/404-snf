import { AnimatePresence, motion } from 'motion/react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import './App.css'
import { useSnfTelemetry } from '@/hooks/useSnfTelemetry'
import '@/i18n'
import { getSnfErrorTranslation } from '@/lib/snfErrors'
import { BottomNav, type Tab } from '@/repose/BottomNav'
import { DeviceTab } from '@/repose/DeviceTab'
import { RecordTab } from '@/repose/RecordTab'
import { StatusTab, type StatusState } from '@/repose/StatusTab'

function AppContent() {
  const [activeTab, setActiveTab] = useState<Tab>('status')
  const [statusState, setStatusState] = useState<StatusState>('work')
  const [paused, setPaused] = useState(false)
  const [notice, setNotice] = useState('')
  const telemetry = useSnfTelemetry(paused)
  const { t } = useTranslation('translation')

  const showNotice = (message: string) => {
    setNotice(message)
    window.setTimeout(() => {
      setNotice('')
    }, 3600)
  }

  const requestConnection = async () => {
    if (telemetry.connected) {
      telemetry.disconnect()
      showNotice(t('notice.disconnected'))
      return
    }
    try {
      await telemetry.connect()
      showNotice(t('notice.connected'))
    } catch (error: unknown) {
      showNotice(t(getSnfErrorTranslation(error)))
    }
  }

  return (
    <div id="container" className="repose-stage">
      <div className="repose-phone">
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
              {activeTab === 'record' && <RecordTab />}
              {activeTab === 'device' && (
                <DeviceTab
                  telemetry={telemetry}
                  paused={paused}
                  onRequestConnection={() => {
                    void requestConnection()
                  }}
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

      <AnimatePresence>
        {notice !== '' && (
          <motion.div
            className="toast"
            role="status"
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0 }}
          >
            {notice}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

function App() {
  return <AppContent />
}

export default App
