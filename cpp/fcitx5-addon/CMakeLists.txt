cmake_minimum_required(VERSION 3.16)

project(vinput_fcitx_bridge VERSION 0.1.0 LANGUAGES CXX)

include(CTest)
include(GNUInstallDirs)
find_package(PkgConfig REQUIRED)
pkg_check_modules(SYSTEMD REQUIRED libsystemd)
find_package(Fcitx5Core QUIET)
find_package(Fcitx5ModuleClipboard QUIET)
find_package(Fcitx5ModuleDBus QUIET)
option(VINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS
       "Probe Fcitx5 dependencies for the retained addon module target" ON)
option(VINPUT_FCITX_BRIDGE_ENABLE_TESTS
       "Build pure C++ bridge smoke tests" ON)
option(VINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE
       "Fail configure if Fcitx5Core is unavailable" OFF)
option(VINPUT_FCITX_BRIDGE_INSTALL_DBUS_SERVICE
       "Install the DBus activation service for the Rust daemon" ON)
option(VINPUT_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE
       "Install the systemd user service for the Rust daemon" ON)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

find_program(CARGO_EXECUTABLE cargo REQUIRED)
get_filename_component(
  VINPUT_REPOSITORY_ROOT
  "${CMAKE_CURRENT_SOURCE_DIR}/../.."
  ABSOLUTE
)
set(
  VINPUT_FCITX_RUST_TARGET_DIR
  "${VINPUT_REPOSITORY_ROOT}/target"
  CACHE PATH "Cargo target directory for the Rust frontend bridge"
)
if(CMAKE_BUILD_TYPE MATCHES "^(Release|RelWithDebInfo|MinSizeRel)$")
  set(VINPUT_FCITX_RUST_CARGO_ARGS --release)
  set(VINPUT_FCITX_RUST_PROFILE release)
else()
  set(VINPUT_FCITX_RUST_CARGO_ARGS)
  set(VINPUT_FCITX_RUST_PROFILE debug)
endif()
set(
  VINPUT_FCITX_RUST_LIBRARY
  "${VINPUT_FCITX_RUST_TARGET_DIR}/${VINPUT_FCITX_RUST_PROFILE}/libvinput_fcitx_ffi.a"
)
file(
  GLOB_RECURSE VINPUT_FCITX_RUST_SOURCES CONFIGURE_DEPENDS
  "${VINPUT_REPOSITORY_ROOT}/crates/vinput-fcitx-core/src/*.rs"
  "${VINPUT_REPOSITORY_ROOT}/crates/vinput-fcitx-dbus/src/*.rs"
  "${VINPUT_REPOSITORY_ROOT}/crates/vinput-fcitx-ffi/src/*.rs"
  "${VINPUT_REPOSITORY_ROOT}/crates/vinput-protocol/src/*.rs"
)
add_custom_command(
  OUTPUT "${VINPUT_FCITX_RUST_LIBRARY}"
  COMMAND
    "${CMAKE_COMMAND}" -E env
    "CARGO_TARGET_DIR=${VINPUT_FCITX_RUST_TARGET_DIR}"
    "${CARGO_EXECUTABLE}" build --locked -p vinput-fcitx-ffi
    ${VINPUT_FCITX_RUST_CARGO_ARGS}
  WORKING_DIRECTORY "${VINPUT_REPOSITORY_ROOT}"
  DEPENDS
    "${VINPUT_REPOSITORY_ROOT}/Cargo.toml"
    "${VINPUT_REPOSITORY_ROOT}/Cargo.lock"
    "${VINPUT_REPOSITORY_ROOT}/crates/vinput-fcitx-core/Cargo.toml"
    "${VINPUT_REPOSITORY_ROOT}/crates/vinput-fcitx-dbus/Cargo.toml"
    "${VINPUT_REPOSITORY_ROOT}/crates/vinput-fcitx-ffi/Cargo.toml"
    "${VINPUT_REPOSITORY_ROOT}/crates/vinput-protocol/Cargo.toml"
    ${VINPUT_FCITX_RUST_SOURCES}
  COMMENT "Building Rust Fcitx frontend bridge"
  VERBATIM
)
add_custom_target(
  vinput_fcitx_rust_ffi_build
  DEPENDS "${VINPUT_FCITX_RUST_LIBRARY}"
)
add_library(vinput_fcitx_rust_ffi STATIC IMPORTED GLOBAL)
set_target_properties(
  vinput_fcitx_rust_ffi
  PROPERTIES
    IMPORTED_LOCATION "${VINPUT_FCITX_RUST_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES
      "${VINPUT_REPOSITORY_ROOT}/crates/vinput-fcitx-ffi/include"
)
add_dependencies(vinput_fcitx_rust_ffi vinput_fcitx_rust_ffi_build)
function(vinput_bridge_install_dir output value fallback)
  if(NOT "${${output}}" STREQUAL "")
    set(candidate "${${output}}")
  elseif(NOT "${value}" STREQUAL "")
    set(candidate "${value}")
  else()
    set(candidate "${fallback}")
  endif()
  if(IS_ABSOLUTE "${candidate}")
      file(RELATIVE_PATH prefix_relative_value "${CMAKE_INSTALL_PREFIX}" "${candidate}")
      if(NOT prefix_relative_value MATCHES "^\\.\\.($|/)")
        set(${output} "${prefix_relative_value}" PARENT_SCOPE)
      else()
        file(RELATIVE_PATH root_relative_value "/" "${candidate}")
        set(${output} "${root_relative_value}" PARENT_SCOPE)
      endif()
  else()
    set(${output} "${candidate}" PARENT_SCOPE)
  endif()
endfunction()

function(vinput_bridge_apply_warnings target)
  if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    target_compile_options(${target} PRIVATE -Wall -Wextra -Werror)
  endif()
endfunction()

add_library(vinput_fcitx_bridge_core STATIC
  src/frontend_bridge.cpp
  src/menu_snapshot.cpp
  src/recognition_payload.cpp
)
add_library(vinput_fcitx_bridge::core ALIAS vinput_fcitx_bridge_core)
target_include_directories(vinput_fcitx_bridge_core
  PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>
)
target_link_libraries(vinput_fcitx_bridge_core PRIVATE vinput_fcitx_rust_ffi)
target_compile_features(vinput_fcitx_bridge_core PUBLIC cxx_std_20)
set_target_properties(vinput_fcitx_bridge_core PROPERTIES POSITION_INDEPENDENT_CODE ON)
vinput_bridge_apply_warnings(vinput_fcitx_bridge_core)

add_library(vinput_fcitx_bridge_sdbus STATIC src/sd_bus_daemon_client.cpp)
add_library(vinput_fcitx_bridge::sdbus ALIAS vinput_fcitx_bridge_sdbus)
target_link_libraries(vinput_fcitx_bridge_sdbus
  PUBLIC
    vinput_fcitx_bridge::core
  PRIVATE
    vinput_fcitx_rust_ffi
)
target_compile_features(vinput_fcitx_bridge_sdbus PUBLIC cxx_std_20)
set_target_properties(vinput_fcitx_bridge_sdbus PROPERTIES POSITION_INDEPENDENT_CODE ON)
vinput_bridge_apply_warnings(vinput_fcitx_bridge_sdbus)

set(VINPUT_DAEMON_EXECUTABLE
    "${CMAKE_INSTALL_FULL_BINDIR}/vinput-daemon"
    CACHE FILEPATH "Installed vinput-daemon executable used by DBus activation")
set(VINPUT_DAEMON_ARGS
    "--dbus"
    CACHE STRING "Arguments used by the DBus activation service for vinput-daemon")
set(VINPUT_DAEMON_ARGS_SUFFIX "")
if(NOT "${VINPUT_DAEMON_ARGS}" STREQUAL "")
  set(VINPUT_DAEMON_ARGS_SUFFIX " ${VINPUT_DAEMON_ARGS}")
endif()
set(VINPUT_DBUS_SYSTEMD_SERVICE_LINE "")
if(VINPUT_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE)
  if("${VINPUT_SYSTEMD_USER_UNIT_DIR}" STREQUAL "")
    pkg_get_variable(VINPUT_SYSTEMD_USER_UNIT_DIR systemd systemduserunitdir)
  endif()
  if("${VINPUT_SYSTEMD_USER_UNIT_DIR}" STREQUAL "")
    set(VINPUT_SYSTEMD_USER_UNIT_DIR "${CMAKE_INSTALL_LIBDIR}/systemd/user")
  endif()
  set(VINPUT_DBUS_SYSTEMD_SERVICE_LINE "SystemdService=vinput-daemon.service")
  configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/../../data/vinput-daemon.service.in"
    vinput-daemon.service
    @ONLY
  )
endif()
configure_file(vinput-addon.conf.in vinput-addon.conf @ONLY)
configure_file(
  "${CMAKE_CURRENT_SOURCE_DIR}/../../data/org.fcitx.Vinput.service.in"
  org.fcitx.Vinput.service
  @ONLY
)


if(Fcitx5Core_FOUND)
  find_package(Gettext REQUIRED)
  find_package(Fcitx5ModuleDBus REQUIRED)
  find_package(Fcitx5ModuleNotifications REQUIRED)
  set(VINPUT_FCITX_BUILD_LOCALEDIR "${CMAKE_CURRENT_BINARY_DIR}/locale")
  set(VINPUT_FCITX_RUNTIME_BUILD_LOCALEDIR
      "${VINPUT_FCITX_BUILD_LOCALEDIR}"
      CACHE STRING "Runtime build-tree locale fallback embedded in the addon")
  set(VINPUT_FCITX_RUNTIME_INSTALL_LOCALEDIR
      "${CMAKE_INSTALL_FULL_LOCALEDIR}"
      CACHE STRING "Runtime installed locale root embedded in the addon")
  set(VINPUT_FCITX_ZH_CN_MO
      "${VINPUT_FCITX_BUILD_LOCALEDIR}/zh_CN/LC_MESSAGES/fcitx5-vinput.mo")
  add_custom_command(
    OUTPUT "${VINPUT_FCITX_ZH_CN_MO}"
    COMMAND ${CMAKE_COMMAND} -E make_directory
            "${VINPUT_FCITX_BUILD_LOCALEDIR}/zh_CN/LC_MESSAGES"
    COMMAND "${GETTEXT_MSGFMT_EXECUTABLE}" -o "${VINPUT_FCITX_ZH_CN_MO}"
            "${CMAKE_CURRENT_SOURCE_DIR}/po/zh_CN.po"
    DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/po/zh_CN.po"
    COMMENT "Compiling fcitx5-vinput zh_CN translations"
    VERBATIM
  )
  add_custom_target(vinput_fcitx_translations ALL
                    DEPENDS "${VINPUT_FCITX_ZH_CN_MO}")
  install(
    FILES "${VINPUT_FCITX_ZH_CN_MO}"
    DESTINATION "${CMAKE_INSTALL_LOCALEDIR}/zh_CN/LC_MESSAGES"
  )

  add_library(vinput_fcitx_bridge_fcitx STATIC
    src/fcitx_i18n.cpp
    src/fcitx_config.cpp
    src/fcitx_daemon_signal_monitor.cpp
    src/fcitx_candidates.cpp
    src/fcitx_key_trigger.cpp
    src/fcitx_menu_filter.cpp
    src/fcitx_menu_projection.cpp
    src/fcitx_notifications.cpp
    src/fcitx_trigger_mode.cpp
    src/fcitx_outcome.cpp
    src/fcitx_selection.cpp
  )
  add_library(vinput_fcitx_bridge::fcitx ALIAS vinput_fcitx_bridge_fcitx)
  target_link_libraries(vinput_fcitx_bridge_fcitx
    PUBLIC
      vinput_fcitx_bridge::core
      Fcitx5::Core
    PRIVATE
      vinput_fcitx_rust_ffi
      Fcitx5::Module::Notifications
  )
  target_compile_features(vinput_fcitx_bridge_fcitx PUBLIC cxx_std_20)
  target_compile_definitions(
    vinput_fcitx_bridge_fcitx
    PUBLIC
      FCITX_GETTEXT_DOMAIN="fcitx5-vinput"
    PRIVATE
      VINPUT_FCITX_BUILD_LOCALEDIR="${VINPUT_FCITX_RUNTIME_BUILD_LOCALEDIR}"
      VINPUT_FCITX_INSTALL_LOCALEDIR="${VINPUT_FCITX_RUNTIME_INSTALL_LOCALEDIR}"
  )
  add_dependencies(vinput_fcitx_bridge_fcitx vinput_fcitx_translations)
  if(Fcitx5Core_VERSION VERSION_GREATER_EQUAL "5.1.9")
    target_compile_definitions(
      vinput_fcitx_bridge_fcitx
      PUBLIC VINPUT_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
    )
  endif()
  set_target_properties(vinput_fcitx_bridge_fcitx PROPERTIES POSITION_INDEPENDENT_CODE ON)
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_fcitx)

  add_library(fcitx5_vinput_addon MODULE
    src/fcitx_addon.cpp
    src/fcitx_addon_menu.cpp
    src/fcitx_addon_factory.cpp
  )
  target_link_libraries(fcitx5_vinput_addon
    PRIVATE
      vinput_fcitx_bridge::sdbus
      vinput_fcitx_bridge::fcitx
      vinput_fcitx_rust_ffi
      Fcitx5::Core
      Fcitx5::Module::DBus
  )
  target_compile_features(fcitx5_vinput_addon PRIVATE cxx_std_20)
  if(Fcitx5ModuleClipboard_FOUND)
    target_link_libraries(fcitx5_vinput_addon PRIVATE Fcitx5::Module::Clipboard)
    target_compile_definitions(fcitx5_vinput_addon PRIVATE VINPUT_FCITX_HAVE_CLIPBOARD)
  endif()
  if(Fcitx5Core_VERSION VERSION_GREATER_EQUAL "5.1.12")
    target_compile_definitions(
      fcitx5_vinput_addon
      PRIVATE VINPUT_FCITX5_CORE_HAVE_ADDON_FACTORY_V2
    )
  endif()
  set_target_properties(fcitx5_vinput_addon PROPERTIES
    PREFIX ""
    OUTPUT_NAME "fcitx5-vinput"
  )
  vinput_bridge_apply_warnings(fcitx5_vinput_addon)

  if(BUILD_TESTING AND VINPUT_FCITX_BRIDGE_ENABLE_TESTS)
  add_executable(
    vinput_fcitx_bridge_daemon_signal_monitor_smoke
    tests/fcitx_daemon_signal_monitor_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_daemon_signal_monitor_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_daemon_signal_monitor_smoke)
  find_program(DBUS_RUN_SESSION_EXECUTABLE dbus-run-session REQUIRED)
  add_test(
    NAME vinput_fcitx_bridge_daemon_signal_monitor_smoke
    COMMAND "${DBUS_RUN_SESSION_EXECUTABLE}" --
            $<TARGET_FILE:vinput_fcitx_bridge_daemon_signal_monitor_smoke>
  )

  add_executable(
    vinput_fcitx_bridge_notifications_smoke
    tests/fcitx_notifications_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_notifications_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_notifications_smoke)
  add_test(
    NAME vinput_fcitx_bridge_notifications_smoke
    COMMAND vinput_fcitx_bridge_notifications_smoke
  )

  add_executable(
    vinput_fcitx_bridge_i18n_smoke
    tests/fcitx_i18n_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_i18n_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  add_dependencies(vinput_fcitx_bridge_i18n_smoke vinput_fcitx_translations)
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_i18n_smoke)
  add_test(
    NAME vinput_fcitx_bridge_i18n_smoke
    COMMAND vinput_fcitx_bridge_i18n_smoke
  )
  set_tests_properties(
    vinput_fcitx_bridge_i18n_smoke
    PROPERTIES ENVIRONMENT
      "LANGUAGE=zh_CN;LC_ALL=zh_CN.utf8;VINPUT_FCITX_LOCALEDIR=${VINPUT_FCITX_BUILD_LOCALEDIR}"
  )

  add_executable(
    vinput_fcitx_bridge_config_smoke
    tests/fcitx_config_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_config_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_config_smoke)
  add_test(
    NAME vinput_fcitx_bridge_config_smoke
    COMMAND vinput_fcitx_bridge_config_smoke
  )
  set_tests_properties(
    vinput_fcitx_bridge_config_smoke
    PROPERTIES ENVIRONMENT
      "LANGUAGE=zh_CN;LC_ALL=zh_CN.utf8;VINPUT_FCITX_LOCALEDIR=${VINPUT_FCITX_BUILD_LOCALEDIR}"
  )

  add_executable(
    vinput_fcitx_bridge_candidates_smoke
    tests/fcitx_candidates_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_candidates_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_candidates_smoke)
  add_test(
    NAME vinput_fcitx_bridge_candidates_smoke
    COMMAND vinput_fcitx_bridge_candidates_smoke
  )

  add_executable(
    vinput_fcitx_bridge_key_trigger_smoke
    tests/fcitx_key_trigger_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_key_trigger_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_key_trigger_smoke)
  add_test(
    NAME vinput_fcitx_bridge_key_trigger_smoke
    COMMAND vinput_fcitx_bridge_key_trigger_smoke
  )

  add_executable(
    vinput_fcitx_bridge_menu_filter_smoke
    tests/fcitx_menu_filter_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_menu_filter_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_menu_filter_smoke)
  add_test(
    NAME vinput_fcitx_bridge_menu_filter_smoke
    COMMAND vinput_fcitx_bridge_menu_filter_smoke
  )

  add_executable(
    vinput_fcitx_bridge_menu_paging_smoke
    tests/fcitx_menu_paging_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_menu_paging_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_menu_paging_smoke)
  add_test(
    NAME vinput_fcitx_bridge_menu_paging_smoke
    COMMAND vinput_fcitx_bridge_menu_paging_smoke
  )

  add_executable(
    vinput_fcitx_bridge_trigger_mode_smoke
    tests/fcitx_trigger_mode_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_trigger_mode_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_trigger_mode_smoke)
  add_test(
    NAME vinput_fcitx_bridge_trigger_mode_smoke
    COMMAND vinput_fcitx_bridge_trigger_mode_smoke
  )

  add_executable(
    vinput_fcitx_bridge_selection_smoke
    tests/fcitx_selection_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_selection_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_selection_smoke)
  add_test(
    NAME vinput_fcitx_bridge_selection_smoke
    COMMAND vinput_fcitx_bridge_selection_smoke
  )

  add_executable(
    vinput_fcitx_bridge_outcome_sink_smoke
    tests/fcitx_outcome_sink_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_outcome_sink_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_outcome_sink_smoke)
  add_test(
    NAME vinput_fcitx_bridge_outcome_sink_smoke
    COMMAND vinput_fcitx_bridge_outcome_sink_smoke
  )

  add_executable(
    vinput_fcitx_bridge_input_context_outcome_smoke
    tests/fcitx_input_context_outcome_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_input_context_outcome_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_input_context_outcome_smoke)
  add_test(
    NAME vinput_fcitx_bridge_input_context_outcome_smoke
    COMMAND vinput_fcitx_bridge_input_context_outcome_smoke
  )

  add_executable(
    vinput_fcitx_addon_dbus_smoke
    tests/fcitx_addon_dbus_smoke.cpp
    src/fcitx_addon.cpp
    src/fcitx_addon_menu.cpp
    src/fcitx_i18n.cpp
    src/fcitx_config.cpp
    src/fcitx_daemon_signal_monitor.cpp
    src/fcitx_key_trigger.cpp
    src/fcitx_menu_filter.cpp
    src/fcitx_menu_projection.cpp
    src/fcitx_notifications.cpp
    src/fcitx_trigger_mode.cpp
    src/fcitx_selection.cpp
  )
  target_compile_definitions(
    vinput_fcitx_addon_dbus_smoke
    PRIVATE
      FCITX_GETTEXT_DOMAIN="fcitx5-vinput"
      VINPUT_FCITX_BUILD_LOCALEDIR="${VINPUT_FCITX_RUNTIME_BUILD_LOCALEDIR}"
      VINPUT_FCITX_INSTALL_LOCALEDIR="${CMAKE_INSTALL_FULL_LOCALEDIR}"
  )
  add_dependencies(vinput_fcitx_addon_dbus_smoke vinput_fcitx_translations)

  target_link_libraries(
    vinput_fcitx_addon_dbus_smoke
    PRIVATE
      vinput_fcitx_bridge::sdbus
      vinput_fcitx_rust_ffi
      Fcitx5::Core
      Fcitx5::Module::DBus
      Fcitx5::Module::Notifications
  )
  vinput_bridge_apply_warnings(vinput_fcitx_addon_dbus_smoke)

  add_executable(
    vinput_fcitx_native_addon_dbus_smoke
    tests/native_fcitx_addon_dbus_smoke.cpp
    src/fcitx_addon.cpp
    src/fcitx_addon_menu.cpp
  )
  target_compile_definitions(
    vinput_fcitx_native_addon_dbus_smoke
    PRIVATE
      VINPUT_FCITX_BUILD_LOCALEDIR="${VINPUT_FCITX_RUNTIME_BUILD_LOCALEDIR}"
      VINPUT_FCITX_INSTALL_LOCALEDIR="${CMAKE_INSTALL_FULL_LOCALEDIR}"
  )
  add_dependencies(vinput_fcitx_native_addon_dbus_smoke vinput_fcitx_translations)
  target_link_libraries(
    vinput_fcitx_native_addon_dbus_smoke
    PRIVATE
      vinput_fcitx_bridge::sdbus
      vinput_fcitx_bridge::fcitx
      vinput_fcitx_rust_ffi
      Fcitx5::Core
      Fcitx5::Module::DBus
      Fcitx5::Module::Notifications
  )
  vinput_bridge_apply_warnings(vinput_fcitx_native_addon_dbus_smoke)
  endif()

  vinput_bridge_install_dir(
    VINPUT_FCITX_MODULE_INSTALL_DIR
    "${FCITX_INSTALL_LIBDIR}/fcitx5"
    "${CMAKE_INSTALL_LIBDIR}/fcitx5"
  )
  vinput_bridge_install_dir(
    VINPUT_FCITX_ADDON_INSTALL_DIR
    "${FCITX_INSTALL_PKGDATADIR}/addon"
    "${CMAKE_INSTALL_DATADIR}/fcitx5/addon"
  )
  install(TARGETS fcitx5_vinput_addon DESTINATION "${VINPUT_FCITX_MODULE_INSTALL_DIR}")
  install(
    FILES "${CMAKE_CURRENT_BINARY_DIR}/vinput-addon.conf"
    DESTINATION "${VINPUT_FCITX_ADDON_INSTALL_DIR}"
    RENAME vinput.conf
  )
  if(VINPUT_FCITX_BRIDGE_INSTALL_DBUS_SERVICE)
    install(
      FILES "${CMAKE_CURRENT_BINARY_DIR}/org.fcitx.Vinput.service"
      DESTINATION "${CMAKE_INSTALL_DATADIR}/dbus-1/services"
    )
  endif()
  if(VINPUT_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE)
    set(VINPUT_SYSTEMD_USER_UNIT_INSTALL_DIR
        "${VINPUT_SYSTEMD_USER_UNIT_DIR}")
    install(
      FILES "${CMAKE_CURRENT_BINARY_DIR}/vinput-daemon.service"
      DESTINATION "${VINPUT_SYSTEMD_USER_UNIT_INSTALL_DIR}"
    )
  endif()
else()
  if(VINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE)
    message(FATAL_ERROR "Fcitx5Core is required for the addon module target")
  endif()
  message(STATUS "Fcitx5Core not found; skipping fcitx5-vinput module target")
endif()

if(VINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS)
  find_package(Fcitx5ModuleDBus QUIET)
  find_package(Fcitx5ModuleClipboard QUIET)
  find_package(Fcitx5ModuleNotifications QUIET)

  if(Fcitx5Core_FOUND AND Fcitx5ModuleDBus_FOUND AND
     Fcitx5ModuleClipboard_FOUND AND Fcitx5ModuleNotifications_FOUND)
    message(STATUS "Fcitx5 addon dependencies found; retained addon sources can link module helpers")
  else()
    message(STATUS "Fcitx5 addon dependencies incomplete; building bridge core and sd-bus client only")
  endif()
endif()

if(BUILD_TESTING AND VINPUT_FCITX_BRIDGE_ENABLE_TESTS)
  function(vinput_bridge_smoke_test target source)
    add_executable(${target} ${source})
    target_link_libraries(${target} PRIVATE vinput_fcitx_bridge::core)
    vinput_bridge_apply_warnings(${target})
    add_test(NAME ${target} COMMAND ${target})
  endfunction()

  add_executable(
    vinput_fcitx_bridge_rust_ffi_smoke
    tests/rust_ffi_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_rust_ffi_smoke
    PRIVATE
      vinput_fcitx_rust_ffi
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_rust_ffi_smoke)
  add_test(
    NAME vinput_fcitx_bridge_rust_ffi_smoke
    COMMAND vinput_fcitx_bridge_rust_ffi_smoke
  )
  vinput_bridge_smoke_test(
    vinput_fcitx_bridge_recognition_payload_smoke
    tests/recognition_payload_smoke.cpp
  )
  target_link_libraries(vinput_fcitx_bridge_recognition_payload_smoke
                        PRIVATE vinput_fcitx_rust_ffi)
  vinput_bridge_smoke_test(
    vinput_fcitx_bridge_frontend_smoke
    tests/frontend_bridge_smoke.cpp
  )

  add_executable(
    vinput_fcitx_bridge_dbus_smoke
    tests/sd_bus_daemon_client_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_dbus_smoke
    PRIVATE
      vinput_fcitx_bridge::sdbus
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_dbus_smoke)

  add_executable(
    vinput_fcitx_bridge_native_dbus_smoke
    tests/native_frontend_bridge_dbus_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_native_dbus_smoke
    PRIVATE
      vinput_fcitx_bridge::sdbus
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_native_dbus_smoke)
endif()
