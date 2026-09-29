cmake_minimum_required(VERSION 3.25)

project(OpenPremier VERSION 0.1.0 LANGUAGES NONE)

find_program(OPENPREMIER_CARGO_EXECUTABLE cargo)

add_custom_target(
  project-status
  COMMAND ${CMAKE_COMMAND} -E echo "OpenPremier 0.1.0 technical alpha: Rust implementation exists."
  COMMAND ${CMAKE_COMMAND} -E echo "Compatibility status: see docs/implementation-status.md."
  VERBATIM
)

if(OPENPREMIER_CARGO_EXECUTABLE)
  add_custom_target(
    openpremier-build
    COMMAND ${OPENPREMIER_CARGO_EXECUTABLE} xtask build --release
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    COMMENT "Build OpenPremier through the configured Rust task runner"
    VERBATIM
  )
  add_custom_target(
    openpremier-test
    COMMAND ${OPENPREMIER_CARGO_EXECUTABLE} xtask test
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    COMMENT "Run the OpenPremier test suite"
    VERBATIM
  )
else()
  message(WARNING "Cargo was not found; Rust build targets are unavailable")
endif()

# CMake is a thin integration layer for native SDK consumers. Rust/Cargo is the primary build
# system; no C++ application target is required.
