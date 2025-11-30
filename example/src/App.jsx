import React, { useState, useRef, useCallback, useEffect } from 'react'
import {
  Box,
  Container,
  VStack,
  HStack,
  Heading,
  Text,
  Button,
  Input,
  Card,
  CardBody,
  CardHeader,
  Badge,
  Code,
  Divider,
  Alert,
  AlertIcon,
  Spinner,
  useToast,
  Flex,
  IconButton,
  Tooltip,
} from '@chakra-ui/react'

// Log entry type
const LogLevel = {
  INFO: 'info',
  SUCCESS: 'success',
  ERROR: 'error',
  WARN: 'warn',
}

function App() {
  const [isConnecting, setIsConnecting] = useState(false)
  const [isConnected, setIsConnected] = useState(false)
  const [isFetching, setIsFetching] = useState(false)
  const [logs, setLogs] = useState([])
  const [result, setResult] = useState(null)
  const [torClient, setTorClient] = useState(null)
  const [wasmModule, setWasmModule] = useState(null)
  const [circuitStatus, setCircuitStatus] = useState('Not connected')
  
  const logsEndRef = useRef(null)
  const toast = useToast()
  
  // Snowflake configuration (Snowflake is the only transport supported in WASM)
  const [snowflakeUrl, setSnowflakeUrl] = useState(
    'wss://snowflake.torproject.net/'
  )
  
  const targetUrl = 'https://api64.ipify.org?format=json'
  
  // Add log entry
  const addLog = useCallback((message, level = LogLevel.INFO) => {
    const timestamp = new Date().toLocaleTimeString()
    setLogs(prev => [...prev, { timestamp, message, level }])
  }, [])
  
  // Auto-scroll logs
  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [logs])
  
  // Load WASM module
  const loadWasm = useCallback(async () => {
    if (wasmModule) return wasmModule
    
    addLog('Loading WASM module...', LogLevel.INFO)
    
    try {
      // Try to import the WASM module from the pkg directory
      const wasm = await import('../pkg/webtor_wasm.js')
      await wasm.default()
      wasm.init()
      
      setWasmModule(wasm)
      addLog('WASM module loaded successfully', LogLevel.SUCCESS)
      return wasm
    } catch (error) {
      addLog(`Failed to load WASM: ${error.message}`, LogLevel.ERROR)
      throw error
    }
  }, [wasmModule, addLog])
  
  // Connect to Tor via Snowflake
  const handleConnect = async () => {
    setIsConnecting(true)
    setLogs([])
    setResult(null)
    
    try {
      addLog('Starting Tor connection via Snowflake...', LogLevel.INFO)
      addLog(`Snowflake URL: ${snowflakeUrl}`, LogLevel.INFO)
      
      const wasm = await loadWasm()
      
      addLog('Creating TorClient with Snowflake bridge...', LogLevel.INFO)
      
      // Create options for Snowflake (the only transport supported in WASM)
      const options = new wasm.TorClientOptions(snowflakeUrl)
        .withCreateCircuitEarly(true)
        .withConnectionTimeout(60000)
        .withCircuitTimeout(180000)
      
      addLog('Establishing Snowflake connection...', LogLevel.INFO)
      addLog('This may take 30-90 seconds on first connection', LogLevel.WARN)
      
      const client = await new wasm.TorClient(options)
      
      addLog('TorClient created, waiting for circuit...', LogLevel.INFO)
      setCircuitStatus('Creating circuit...')
      
      await client.waitForCircuit()
      
      setTorClient(client)
      setIsConnected(true)
      setCircuitStatus('Connected')
      addLog('Tor circuit established successfully!', LogLevel.SUCCESS)
      
      toast({
        title: 'Connected to Tor',
        description: 'Snowflake circuit is ready',
        status: 'success',
        duration: 3000,
      })
    } catch (error) {
      addLog(`Connection failed: ${error.message || error}`, LogLevel.ERROR)
      setCircuitStatus('Connection failed')
      
      toast({
        title: 'Connection Failed',
        description: error.message || 'Failed to connect to Tor',
        status: 'error',
        duration: 5000,
      })
    } finally {
      setIsConnecting(false)
    }
  }
  
  // Disconnect from Tor
  const handleDisconnect = async () => {
    if (torClient) {
      addLog('Closing Tor connection...', LogLevel.INFO)
      await torClient.close()
      setTorClient(null)
    }
    setIsConnected(false)
    setCircuitStatus('Disconnected')
    setResult(null)
    addLog('Tor connection closed', LogLevel.SUCCESS)
  }
  
  // Fetch IP through Tor
  const handleFetch = async () => {
    if (!torClient) {
      toast({
        title: 'Not connected',
        description: 'Please connect to Tor first',
        status: 'warning',
        duration: 3000,
      })
      return
    }
    
    setIsFetching(true)
    setResult(null)
    
    try {
      addLog(`Fetching ${targetUrl} through Tor...`, LogLevel.INFO)
      const startTime = Date.now()
      
      const response = await torClient.fetch(targetUrl)
      const elapsed = Date.now() - startTime
      
      addLog(`Response received in ${elapsed}ms`, LogLevel.SUCCESS)
      addLog(`Status: ${response.status}`, LogLevel.INFO)
      
      const text = response.text()
      addLog(`Response body: ${text}`, LogLevel.INFO)
      
      try {
        const json = JSON.parse(text)
        setResult({
          ip: json.ip,
          elapsed,
          status: response.status,
        })
        addLog(`Your Tor exit IP: ${json.ip}`, LogLevel.SUCCESS)
      } catch {
        setResult({
          raw: text,
          elapsed,
          status: response.status,
        })
      }
    } catch (error) {
      addLog(`Fetch failed: ${error.message || error}`, LogLevel.ERROR)
      
      toast({
        title: 'Fetch Failed',
        description: error.message || 'Failed to fetch through Tor',
        status: 'error',
        duration: 5000,
      })
    } finally {
      setIsFetching(false)
    }
  }
  
  // Clear logs
  const handleClearLogs = () => {
    setLogs([])
  }
  
  return (
    <Box minH="100vh" bg="gray.900" py={8}>
      <Container maxW="container.xl">
        <VStack spacing={6} align="stretch">
          {/* Header */}
          <Box textAlign="center" mb={2}>
            <Flex justify="center" align="center" gap={3} mb={2}>
              <img src="/tor.svg" alt="Tor" style={{ width: 48, height: 48 }} />
              <Heading size="xl" color="purple.400">
                Webtor HTTP Test
              </Heading>
            </Flex>
            <Text color="gray.400">
              Test HTTP requests through Tor using WebTunnel
            </Text>
          </Box>
          
          {/* Main content - two columns */}
          <Flex gap={6} direction={{ base: 'column', lg: 'row' }}>
            {/* Left column - Connection & Request */}
            <VStack flex={1} spacing={4} align="stretch">
              {/* Connection Status Card */}
              <Card bg="gray.800" borderColor="gray.700" borderWidth={1}>
                <CardHeader pb={2}>
                  <Flex justify="space-between" align="center">
                    <Heading size="md" color="white">
                      Connection Status
                    </Heading>
                    <Badge
                      colorScheme={isConnected ? 'green' : isConnecting ? 'yellow' : 'gray'}
                      fontSize="sm"
                      px={3}
                      py={1}
                      borderRadius="full"
                    >
                      {circuitStatus}
                    </Badge>
                  </Flex>
                </CardHeader>
                <CardBody pt={2}>
                  <VStack spacing={4} align="stretch">
                    <Box>
                      <Text color="gray.400" fontSize="sm" mb={1}>
                        Snowflake Broker URL
                      </Text>
                      <Input
                        value={snowflakeUrl}
                        onChange={(e) => setSnowflakeUrl(e.target.value)}
                        placeholder="wss://snowflake.torproject.net/"
                        bg="gray.700"
                        border="none"
                        size="sm"
                        isDisabled={isConnected || isConnecting}
                        _focus={{ borderColor: 'purple.400', boxShadow: 'none' }}
                      />
                    </Box>
                    
                    <HStack>
                      {!isConnected ? (
                        <Button
                          colorScheme="purple"
                          onClick={handleConnect}
                          isLoading={isConnecting}
                          loadingText="Connecting..."
                          flex={1}
                        >
                          Enable Webtor
                        </Button>
                      ) : (
                        <Button
                          colorScheme="red"
                          variant="outline"
                          onClick={handleDisconnect}
                          flex={1}
                        >
                          Disconnect
                        </Button>
                      )}
                    </HStack>
                  </VStack>
                </CardBody>
              </Card>
              
              {/* HTTP Request Card */}
              <Card bg="gray.800" borderColor="gray.700" borderWidth={1}>
                <CardHeader pb={2}>
                  <Heading size="md" color="white">
                    HTTP Request
                  </Heading>
                </CardHeader>
                <CardBody pt={2}>
                  <VStack spacing={4} align="stretch">
                    <Box>
                      <Text color="gray.400" fontSize="sm" mb={1}>
                        Target URL
                      </Text>
                      <Code
                        display="block"
                        p={3}
                        bg="gray.700"
                        borderRadius="md"
                        color="green.300"
                        fontSize="sm"
                      >
                        {targetUrl}
                      </Code>
                    </Box>
                    
                    <Button
                      colorScheme="green"
                      onClick={handleFetch}
                      isLoading={isFetching}
                      loadingText="Fetching..."
                      isDisabled={!isConnected}
                    >
                      Fetch My Tor IP
                    </Button>
                    
                    {result && (
                      <Alert
                        status="success"
                        bg="green.900"
                        borderRadius="md"
                      >
                        <AlertIcon />
                        <Box>
                          <Text fontWeight="bold">
                            {result.ip ? `Your Tor Exit IP: ${result.ip}` : 'Response received'}
                          </Text>
                          <Text fontSize="sm" color="gray.300">
                            Status: {result.status} | Time: {result.elapsed}ms
                          </Text>
                          {result.raw && (
                            <Code mt={2} fontSize="xs" display="block" p={2}>
                              {result.raw}
                            </Code>
                          )}
                        </Box>
                      </Alert>
                    )}
                  </VStack>
                </CardBody>
              </Card>
            </VStack>
            
            {/* Right column - Logs */}
            <Card bg="gray.800" borderColor="gray.700" borderWidth={1} flex={1}>
              <CardHeader pb={2}>
                <Flex justify="space-between" align="center">
                  <Heading size="md" color="white">
                    Connection Logs
                  </Heading>
                  <Button size="sm" variant="ghost" onClick={handleClearLogs}>
                    Clear
                  </Button>
                </Flex>
              </CardHeader>
              <CardBody pt={2}>
                <Box
                  bg="gray.900"
                  borderRadius="md"
                  p={3}
                  h={{ base: '300px', lg: '100%' }}
                  minH="400px"
                  overflowY="auto"
                  fontFamily="mono"
                  fontSize="xs"
                >
                  {logs.length === 0 ? (
                    <Text color="gray.500">
                      Logs will appear here when you connect...
                    </Text>
                  ) : (
                    logs.map((log, i) => (
                      <Box key={i} mb={1}>
                        <Text
                          as="span"
                          color="gray.500"
                          mr={2}
                        >
                          [{log.timestamp}]
                        </Text>
                        <Text
                          as="span"
                          color={
                            log.level === LogLevel.ERROR
                              ? 'red.400'
                              : log.level === LogLevel.SUCCESS
                              ? 'green.400'
                              : log.level === LogLevel.WARN
                              ? 'yellow.400'
                              : 'gray.300'
                          }
                        >
                          {log.message}
                        </Text>
                      </Box>
                    ))
                  )}
                  <div ref={logsEndRef} />
                </Box>
              </CardBody>
            </Card>
          </Flex>
          
          {/* Info Footer */}
          <Box textAlign="center" color="gray.500" fontSize="sm">
            <Text>
              Powered by webtor-rs • Snowflake pluggable transport
            </Text>
            <Text mt={1}>
              First connection may take 30-90 seconds while consensus is fetched
            </Text>
          </Box>
        </VStack>
      </Container>
    </Box>
  )
}

export default App
