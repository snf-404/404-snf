interface BluetoothRequestDeviceOptions {
  filters: Array<{ services: string[] }>
}

interface Bluetooth {
  requestDevice(options: BluetoothRequestDeviceOptions): Promise<BluetoothDevice>
}

interface BluetoothDevice extends EventTarget {
  readonly name?: string
  readonly gatt?: BluetoothRemoteGATTServer
}

interface BluetoothRemoteGATTServer {
  readonly connected: boolean
  connect(): Promise<BluetoothRemoteGATTServer>
  disconnect(): void
  getPrimaryService(service: string): Promise<BluetoothRemoteGATTService>
}

interface BluetoothRemoteGATTService {
  getCharacteristic(characteristic: string): Promise<BluetoothRemoteGATTCharacteristic>
}

interface BluetoothRemoteGATTCharacteristic extends EventTarget {
  readonly value?: DataView
  readValue(): Promise<DataView>
  startNotifications(): Promise<BluetoothRemoteGATTCharacteristic>
  writeValueWithResponse(value: BufferSource): Promise<void>
}

interface Navigator {
  readonly bluetooth?: Bluetooth
}
